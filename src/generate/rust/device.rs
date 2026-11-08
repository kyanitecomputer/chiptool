use std::fmt::Write as _;

use anyhow::Result;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

use crate::ir::*;
use crate::util;

use super::{sorted, with_defmt_cfg_attr, RustArch};

/// Generate the `device.x` linker script fragment (PROVIDE entries).
///
/// For Cortex-M this emits `PROVIDE(IRQ = DefaultHandler)` entries consumed by
/// `cortex-m-rt`'s `device.x` include.  For RISC-V `riscv-rt 0.12` has no
/// equivalent mechanism, so an empty string is returned.
pub fn render_device_x(_ir: &IR, d: &Device, arch: &RustArch) -> Result<String> {
    match arch {
        RustArch::CortexM => {
            let mut device_x = String::new();
            for i in sorted(&d.interrupts, |i| i.value) {
                writeln!(&mut device_x, "PROVIDE({} = DefaultHandler);", i.name)?;
            }
            Ok(device_x)
        }
        RustArch::RiscV | RustArch::Bare => Ok(String::new()),
    }
}

pub fn render(opts: &super::Options, ir: &IR, d: &Device, path: &str) -> Result<TokenStream> {
    let mut out = TokenStream::new();

    out.extend(render_interrupt_enum(opts, d)?);
    out.extend(render_peripherals(ir, d, path)?);

    // Arch-specific NVIC constant (Cortex-M only)
    if opts.arch() == &RustArch::CortexM {
        if let Some(nvic_priority_bits) = d.nvic_priority_bits {
            let bits = util::unsuffixed(u64::from(nvic_priority_bits));
            out.extend(quote! {
                /// Number of priority bits implemented by the NVIC
                #[cfg(feature = "rt")]
                pub const NVIC_PRIO_BITS: u8 = #bits;
            });
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Interrupt enum — shared structure, arch-specific trait impl
// ---------------------------------------------------------------------------

fn render_interrupt_enum(opts: &super::Options, d: &Device) -> Result<TokenStream> {
    let span = Span::call_site();
    let mut interrupts = TokenStream::new();
    let mut names: Vec<Ident> = vec![];

    for i in sorted(&d.interrupts, |i| i.value) {
        let name_uc = Ident::new(&i.name, span);
        let description = format!(
            "{} - {}",
            i.value,
            i.description
                .as_ref()
                .map(|s| util::respace(s))
                .as_ref()
                .map(|s| util::escape_brackets(s))
                .unwrap_or_else(|| i.name.clone())
        );
        let value = util::unsuffixed(i.value as u64);

        interrupts.extend(quote! {
            #[doc = #description]
            #name_uc = #value,
        });
        names.push(name_uc);
    }

    let derive_defmt = with_defmt_cfg_attr(&opts.defmt, quote! { derive(defmt::Format) });
    let trait_impl = render_interrupt_trait_impl(opts, d);
    let vectors = render_vector_table(opts, d, &names)?;

    let rt_use = match opts.arch() {
        RustArch::CortexM => quote! {
            #[cfg(feature = "rt")]
            pub use cortex_m_rt::interrupt;
            #[cfg(feature = "rt")]
            pub use Interrupt as interrupt;
        },
        RustArch::RiscV => {
            // riscv-rt 0.12 does not re-export an `interrupt` attribute macro.
            // RISC-V interrupt handlers are registered via #[export_name] or
            // riscv-rt-macros::external_interrupt, neither of which is
            // re-exported from riscv_rt.  Only re-export the type alias.
            quote! {
                #[cfg(feature = "rt")]
                pub use Interrupt as interrupt;
            }
        }
        RustArch::Bare => TokenStream::new(),
    };

    Ok(quote! {
        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        #derive_defmt
        pub enum Interrupt {
            #interrupts
        }

        #trait_impl
        #vectors
        #rt_use
    })
}

/// Generate the `unsafe impl <Trait> for Interrupt` block, arch-specific.
fn render_interrupt_trait_impl(opts: &super::Options, d: &Device) -> TokenStream {
    match opts.arch() {
        RustArch::CortexM => quote! {
            unsafe impl cortex_m::interrupt::InterruptNumber for Interrupt {
                #[inline(always)]
                fn number(self) -> u16 {
                    self as u16
                }
            }
        },
        RustArch::RiscV => {
            // riscv-pac 0.2 InterruptNumber requires three items:
            //   const MAX_INTERRUPT_NUMBER: usize
            //   fn number(self) -> usize
            //   fn from_number(value: usize) -> riscv::result::Result<Self>
            let span = Span::call_site();
            let mut match_arms = TokenStream::new();
            let mut max_num: u32 = 0;

            for i in sorted(&d.interrupts, |i| i.value) {
                let name_uc = Ident::new(&i.name, span);
                let value = i.value as usize;
                match_arms.extend(quote! { #value => Ok(Interrupt::#name_uc), });
                if i.value > max_num {
                    max_num = i.value;
                }
            }

            if d.interrupts.is_empty() {
                quote! {
                    unsafe impl riscv::InterruptNumber for Interrupt {
                        const MAX_INTERRUPT_NUMBER: usize = 0;
                        #[inline(always)]
                        fn number(self) -> usize { unreachable!() }
                        #[inline(always)]
                        fn from_number(value: usize) -> riscv::result::Result<Self> {
                            Err(riscv::result::Error::InvalidVariant(value))
                        }
                    }
                    unsafe impl riscv::ExternalInterruptNumber for Interrupt {}
                }
            } else {
                let max_num = max_num as usize;
                quote! {
                    unsafe impl riscv::InterruptNumber for Interrupt {
                        const MAX_INTERRUPT_NUMBER: usize = #max_num;
                        #[inline(always)]
                        fn number(self) -> usize { self as usize }
                        #[inline(always)]
                        fn from_number(value: usize) -> riscv::result::Result<Self> {
                            match value {
                                #match_arms
                                _ => Err(riscv::result::Error::InvalidVariant(value)),
                            }
                        }
                    }
                    unsafe impl riscv::ExternalInterruptNumber for Interrupt {}
                }
            }
        }
        RustArch::Bare => TokenStream::new(),
    }
}

/// Generate the `#[cfg(feature = "rt")]` vector table, arch-specific.
fn render_vector_table(
    opts: &super::Options,
    d: &Device,
    names: &[Ident],
) -> Result<TokenStream> {
    let span = Span::call_site();

    match opts.arch() {
        RustArch::CortexM => {
            // Cortex-M: sparse Vector union array in .vector_table.interrupts.
            // Gaps (reserved entries) must be filled with { _reserved: 0 }.
            let mut vectors = TokenStream::new();
            let mut pos: u32 = 0;
            for i in sorted(&d.interrupts, |i| i.value) {
                while pos < i.value {
                    vectors.extend(quote!(Vector { _reserved: 0 },));
                    pos += 1;
                }
                pos += 1;
                let name_uc = Ident::new(&i.name, span);
                vectors.extend(quote!(Vector { _handler: #name_uc },));
            }
            let n = util::unsuffixed(pos as u64);

            Ok(quote! {
                #[cfg(feature = "rt")]
                mod _vectors {
                    unsafe extern "C" {
                        #(fn #names();)*
                    }

                    pub union Vector {
                        _handler: unsafe extern "C" fn(),
                        _reserved: u32,
                    }

                    #[unsafe(link_section = ".vector_table.interrupts")]
                    #[unsafe(no_mangle)]
                    pub static __INTERRUPTS: [Vector; #n] = [
                        #vectors
                    ];
                }
            })
        }
        RustArch::RiscV => {
            // RISC-V: flat function-pointer array exported as
            // `_external_interrupt_handlers` for `riscv-rt` PLIC dispatch.
            // No gap-filling is needed; the array is indexed by interrupt
            // number so entries must be contiguous from 0.
            //
            // NOTE: This symbol name is specific to PLIC-based dispatch.
            // Chips with an ibex HLIC (hart-local interrupt controller) and
            // no PLIC will not reference this table — it becomes dead code.
            // A future arch sub-variant (e.g. RiscVHlic) may be needed to
            // support alternative dispatch mechanisms.
            let mut handlers = TokenStream::new();
            let mut pos: u32 = 0;
            for i in sorted(&d.interrupts, |i| i.value) {
                // Fill gaps with a no-op default handler
                while pos < i.value {
                    handlers.extend(quote!(DefaultHandler,));
                    pos += 1;
                }
                pos += 1;
                let name_uc = Ident::new(&i.name, span);
                handlers.extend(quote!(#name_uc,));
            }
            let n = util::unsuffixed(pos as u64);

            Ok(quote! {
                #[cfg(feature = "rt")]
                mod _vectors {
                    unsafe extern "C" {
                        #(fn #names();)*
                        fn DefaultHandler();
                    }

                    pub type ExternalInterruptHandler = unsafe extern "C" fn();

                    #[unsafe(export_name = "_external_interrupt_handlers")]
                    pub static __EXTERNAL_INTERRUPTS: [ExternalInterruptHandler; #n] = [
                        #handlers
                    ];
                }
            })
        }
        RustArch::Bare => Ok(TokenStream::new()),
    }
}

// ---------------------------------------------------------------------------
// Peripheral constants — identical for all architectures
// ---------------------------------------------------------------------------

fn render_peripherals(ir: &IR, d: &Device, path: &str) -> Result<TokenStream> {
    let span = Span::call_site();
    let mut peripherals = TokenStream::new();

    for p in sorted(&d.peripherals, |p| p.base_address) {
        let name = Ident::new(&p.name, span);
        let address = util::hex_usize(p.base_address);
        let doc = util::doc(&p.description);

        if let Some(block_name) = &p.block {
            let _b = ir.blocks.get(block_name);
            let ty = util::relative_path(block_name, path);
            peripherals.extend(quote! {
                #doc
                pub const #name: #ty = unsafe { #ty::from_ptr(#address as _) };
            });
        } else {
            peripherals.extend(quote! {
                #doc
                pub const #name: *mut () = #address as _;
            });
        }
    }

    Ok(peripherals)
}
