use anyhow::Result;
use crate::ir::*;
use super::COptions;
use super::block::typedef_name;

/// Render a device header with peripheral instance macros and IRQ number defines.
///
/// Includes a `#include` for every block header whose typedef is referenced,
/// so users only need to include this file.
pub fn render(_ir: &IR, device: &Device, device_name: &str, _opts: &COptions) -> Result<String> {
    let mut out = String::new();

    // Include guard
    let guard = format!("DEVICE_{}_H", device_name.to_uppercase().replace("::", "_"));
    out.push_str(&format!("#ifndef {}\n#define {}\n\n", guard, guard));
    out.push_str("#include <stdint.h>\n");

    // Emit one #include per distinct block header referenced by this device's peripherals.
    // Collect unique block names first to avoid duplicate includes.
    let mut seen_blocks: Vec<String> = Vec::new();
    for periph in &device.peripherals {
        if let Some(block_name) = &periph.block {
            if !seen_blocks.iter().any(|b| b == block_name) {
                seen_blocks.push(block_name.clone());
            }
        }
    }
    for block_name in &seen_blocks {
        let header = format!("pac/{}.h", block_name.to_lowercase().replace("::", "_"));
        out.push_str(&format!("#include \"{}\"\n", header));
    }
    out.push('\n');

    // Peripheral instance macros — one BASE + one typed-pointer macro per peripheral.
    if !device.peripherals.is_empty() {
        out.push_str("/* Peripheral instances */\n");
        for periph in &device.peripherals {
            if let Some(block_name) = &periph.block {
                let name = periph.name.to_uppercase();
                let td = typedef_name(block_name);
                if let Some(desc) = &periph.description {
                    out.push_str(&format!("/* {} */\n", desc));
                }
                out.push_str(&format!(
                    "#define {}_BASE (0x{:08x}U)\n",
                    name, periph.base_address
                ));
                out.push_str(&format!(
                    "#define {} (({} *){}_BASE)\n",
                    name, td, name
                ));
            }
        }
        out.push('\n');
    }

    // IRQ number defines
    if !device.interrupts.is_empty() {
        out.push_str("/* IRQ numbers */\n");
        for interrupt in &device.interrupts {
            if let Some(desc) = &interrupt.description {
                out.push_str(&format!("/* {} */\n", desc));
            }
            out.push_str(&format!(
                "#define {}_IRQn ({}U)\n",
                interrupt.name.to_uppercase(), interrupt.value
            ));
        }
        out.push('\n');
    }

    out.push_str(&format!("#endif /* {} */\n", guard));
    Ok(out)
}
