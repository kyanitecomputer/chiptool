mod block;
mod device;
mod enumm;
mod fieldset;

use anyhow::Result;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use std::collections::BTreeMap;
use std::str::FromStr;

use crate::ir::*;
use super::CodeGenerator;

pub use block::render as block_render;
pub use device::{render as device_render, render_device_x};
pub use enumm::render as enumm_render;
pub use fieldset::render as fieldset_render;

pub const COMMON_MODULE: &str = include_str!("common.rs");

// --- Rust Generator ---

pub struct RustGenerator;

impl CodeGenerator for RustGenerator {
    type Output = TokenStream;
    type Options = Options;

    fn generate(&self, ir: &IR, opts: &Self::Options) -> Result<Vec<(String, Self::Output)>> {
        let token_stream = render(ir, opts)?;
        Ok(vec![("lib.rs".to_string(), token_stream)])
    }
}

// --- Module tree for Rust code generation ---

struct Module {
    items: TokenStream,
    children: BTreeMap<String, Module>,
}

impl Module {
    fn new() -> Self {
        Self {
            items: quote!(),
            children: BTreeMap::new(),
        }
    }

    fn get_by_path(&mut self, path: &[&str]) -> &mut Module {
        if path.is_empty() {
            return self;
        }

        self.children
            .entry(path[0].to_owned())
            .or_insert_with(Module::new)
            .get_by_path(&path[1..])
    }

    fn render(&self) -> Result<TokenStream> {
        let span = Span::call_site();

        let mut res = TokenStream::new();
        res.extend(self.items.clone());

        for (name, module) in sorted_map(&self.children, |name, _| name.clone()) {
            let name = Ident::new(name, span);
            let contents = module.render()?;
            res.extend(quote! {
                pub mod #name {
                    #contents
                }
            });
        }
        Ok(res)
    }
}

// --- Options ---

#[derive(Debug, Default)]
pub enum CommonModule {
    #[default]
    Builtin,
    External(TokenStream),
}

/// Target CPU architecture for the Rust code generator.
///
/// Only affects interrupt/vector-table code in the device module.
/// All other generated code (blocks, fieldsets, enums, common) is
/// architecture-agnostic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RustArch {
    /// ARM Cortex-M — uses `cortex_m` / `cortex_m_rt`, NVIC vector table
    /// in `.vector_table.interrupts`, and emits `NVIC_PRIO_BITS`.
    #[default]
    CortexM,
    /// RISC-V — uses `riscv` / `riscv_rt`, exports
    /// `_external_interrupt_handlers` for PLIC dispatch.
    RiscV,
    /// No interrupt runtime — only the `Interrupt` enum. For application
    /// cores whose firmware drives its own interrupt controller (VIC, GIC,
    /// AIA) and pulls in neither `cortex_m` nor `riscv`.
    Bare,
}

#[derive(Debug)]
pub struct Options {
    common_module: CommonModule,
    defmt: DefmtOption,
    no_std: bool,
    arch: RustArch,
}

#[derive(Debug, Clone)]
pub enum DefmtOption {
    Disabled,
    Feature(String),
    Enabled,
}

impl Default for Options {
    fn default() -> Self {
        Self::new()
    }
}

impl Options {
    pub fn new() -> Self {
        Self {
            common_module: CommonModule::Builtin,
            defmt: DefmtOption::Feature("defmt".to_owned()),
            no_std: true,
            arch: RustArch::CortexM,
        }
    }

    pub fn with_common_module(mut self, common_module: CommonModule) -> Self {
        self.common_module = common_module;
        self
    }

    pub fn with_defmt(mut self, defmt: DefmtOption) -> Self {
        self.defmt = defmt;
        self
    }

    pub fn defmt(&self) -> &DefmtOption {
        &self.defmt
    }

    pub fn with_skip_no_std(mut self, skip: bool) -> Self {
        self.no_std = !skip;
        self
    }

    pub fn with_arch(mut self, arch: RustArch) -> Self {
        self.arch = arch;
        self
    }

    pub fn arch(&self) -> &RustArch {
        &self.arch
    }

    fn common_path(&self) -> TokenStream {
        match &self.common_module {
            CommonModule::Builtin => TokenStream::from_str("crate::common").unwrap(),
            CommonModule::External(path) => path.clone(),
        }
    }
}

// --- Main render function (Rust-specific) ---

pub fn render(ir: &IR, opts: &Options) -> Result<TokenStream> {
    let mut root = Module::new();
    root.items = TokenStream::new();

    let commit_info = {
        let tmp = include_str!(concat!(env!("OUT_DIR"), "/commit-info.txt"));
        if tmp.is_empty() {
            " (untracked)"
        } else {
            tmp
        }
    };

    let doc = format!(
        "Peripheral access API (generated using chiptool v{}{})",
        env!("CARGO_PKG_VERSION"),
        commit_info
    );

    root.items.extend(quote!(
        #![allow(non_camel_case_types)]
        #![allow(non_snake_case)]
        #![allow(non_upper_case_globals)]
        #![doc=#doc]
    ));

    if opts.no_std {
        root.items.extend(quote!(
            #![no_std]
        ));
    }

    for (p, d) in sorted_map(&ir.devices, |name, _| name.clone()) {
        let (mods, _) = split_path(p);
        root.get_by_path(&mods)
            .items
            .extend(device_render(opts, ir, d, p)?);
    }

    for (p, b) in sorted_map(&ir.blocks, |name, _| name.clone()) {
        let (mods, _) = split_path(p);
        root.get_by_path(&mods)
            .items
            .extend(block_render(opts, ir, b, p)?);
    }

    for (p, fs) in sorted_map(&ir.fieldsets, |name, _| name.clone()) {
        let (mods, _) = split_path(p);
        root.get_by_path(&mods)
            .items
            .extend(fieldset_render(opts, ir, fs, p)?);
    }

    for (p, e) in sorted_map(&ir.enums, |name, _| name.clone()) {
        let (mods, _) = split_path(p);
        root.get_by_path(&mods)
            .items
            .extend(enumm_render(opts, ir, e, p)?);
    }

    match &opts.common_module {
        CommonModule::Builtin => {
            let tokens = TokenStream::from_str(COMMON_MODULE).unwrap();
            let module = root.get_by_path(&["common"]);
            module.items = TokenStream::new();
            module.items.extend(tokens);
        }
        CommonModule::External(_) => {}
    }

    root.render()
}

// --- Helper functions ---

fn split_path(s: &str) -> (Vec<&str>, &str) {
    let mut v: Vec<&str> = s.split("::").collect();
    let n = v.pop().unwrap();
    (v, n)
}

fn process_array(array: &Array) -> (usize, TokenStream) {
    match array {
        Array::Regular(array) => {
            let len = array.len as usize;
            let stride = array.stride as usize;
            let offs_expr = quote!(n*#stride);
            (len, offs_expr)
        }
        Array::Cursed(array) => {
            let len = array.offsets.len();
            let offsets = array
                .offsets
                .iter()
                .map(|&x| x as usize)
                .collect::<Vec<_>>();
            let offs_expr = quote!(([#(#offsets),*][n] as usize));
            (len, offs_expr)
        }
    }
}

fn sorted<'a, T: 'a, F, Z>(
    v: impl IntoIterator<Item = &'a T>,
    by: F,
) -> impl IntoIterator<Item = &'a T>
where
    F: Fn(&T) -> Z,
    Z: Ord,
{
    let mut v = v.into_iter().collect::<Vec<_>>();
    v.sort_by_key(|v| by(*v));
    v
}

fn sorted_map<'a, K: 'a, V: 'a, F, Z>(
    v: impl IntoIterator<Item = (&'a K, &'a V)>,
    by: F,
) -> impl IntoIterator<Item = (&'a K, &'a V)>
where
    F: Fn(&K, &V) -> Z,
    Z: Ord,
{
    let mut v = v.into_iter().collect::<Vec<_>>();
    v.sort_by_key(|&(k, v)| by(k, v));
    v
}

fn with_defmt_cfg<F>(defmt: &DefmtOption, f: F) -> Option<TokenStream>
where
    F: FnOnce() -> TokenStream,
{
    match defmt {
        DefmtOption::Disabled => None,
        DefmtOption::Feature(feature) => {
            let body = f();
            Some(quote! {
                #[cfg(feature = #feature)]
                #body
            })
        }
        DefmtOption::Enabled => Some(f()),
    }
}

fn with_defmt_cfg_attr(defmt: &DefmtOption, attr: TokenStream) -> Option<TokenStream> {
    match defmt {
        DefmtOption::Disabled => None,
        DefmtOption::Feature(feature) => Some(quote! { #[cfg_attr(feature = #feature, #attr)] }),
        DefmtOption::Enabled => Some(quote! { #[#attr] }),
    }
}
