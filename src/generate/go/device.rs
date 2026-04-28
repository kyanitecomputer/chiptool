use anyhow::Result;
use crate::ir::*;
use super::GoOptions;

/// Render a device's peripheral instances and IRQ constants as Go const/var blocks.
///
/// Does NOT emit a package declaration — the caller in mod.rs handles the file
/// header so multiple devices can be merged into one device.gen.go without
/// duplicate package lines.
pub fn render(_ir: &IR, device: &Device, _opts: &GoOptions) -> Result<String> {
    let mut out = String::new();

    // One constant per peripheral base address.
    // Use SCREAMING_SNAKE_CASE_BASE for tamago naming convention.
    // Use uint64 literal for addresses > 32-bit (e.g. AST2700 DRAM at 0x400000000).
    if !device.peripherals.is_empty() {
        out.push_str("// Peripheral base addresses\nconst (\n");
        for periph in &device.peripherals {
            let const_name = periph.name.to_uppercase() + "_BASE";
            let desc = periph.description.as_deref().unwrap_or(&periph.name);
            if periph.base_address > 0xFFFF_FFFF {
                out.push_str(&format!(
                    "    // {} {}\n    {} = uint64(0x{:x})\n",
                    const_name, desc, const_name, periph.base_address
                ));
            } else {
                out.push_str(&format!(
                    "    // {} {}\n    {} = 0x{:x}\n",
                    const_name, desc, const_name, periph.base_address
                ));
            }
        }
        out.push_str(")\n\n");
    }

    // IRQ numbers as untyped integer constants.
    // Named SCREAMING_SNAKE_CASE_IRQ.
    if !device.interrupts.is_empty() {
        out.push_str("// IRQ numbers\nconst (\n");
        for interrupt in &device.interrupts {
            let const_name = interrupt.name.to_uppercase() + "_IRQ";
            let desc = interrupt.description.as_deref().unwrap_or(&interrupt.name);
            out.push_str(&format!(
                "    // {} {}\n    {} = {}\n",
                const_name, desc, const_name, interrupt.value
            ));
        }
        out.push_str(")\n");
    }

    Ok(out)
}
