mod block;
mod fieldset;
mod enumm;
mod device;

use anyhow::Result;
use crate::ir::*;
use super::CodeGenerator;

/// Target CPU architecture for C code generation.
///
/// Determines the interrupt table format and any arch-specific macros
/// emitted in the device header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CArch {
    /// ARM Cortex-M (NVIC-style IRQn_Type enum)
    #[default]
    CortexM,
    /// ColdFire V1 (CVIC dispatch table, ast-cf-runtime compatible)
    ColdFire,
    /// RISC-V (PLIC / HLIC — interrupt defines only, no dispatch table)
    RiscV,
}

/// Options for C code generation (matching ast-cf-runtime patterns)
pub struct COptions {
    /// Prefix for include guards (e.g., "PAC_" → UART_H becomes PAC_UART_H)
    pub include_guard_prefix: String,
    /// Target CPU architecture — affects IRQ table format in device header.
    pub arch: CArch,
}

impl Default for COptions {
    fn default() -> Self {
        Self {
            include_guard_prefix: "PAC_".to_string(),
            arch: CArch::default(),
        }
    }
}

pub struct CGenerator;

impl CodeGenerator for CGenerator {
    type Output = String;
    type Options = COptions;

    fn generate(&self, ir: &IR, opts: &Self::Options) -> Result<Vec<(String, Self::Output)>> {
        let mut files = Vec::new();

        // Generate per-peripheral headers from blocks (output to pac/)
        for (name, block) in &ir.blocks {
            let filename = format!("pac/{}.h", name.to_lowercase().replace("::", "_"));
            let content = block::render(ir, block, name, opts)?;
            files.push((filename, content));
        }

        // Generate device headers with peripheral instances
        for (name, device) in &ir.devices {
            let filename = format!("device_{}.h", name.to_lowercase().replace("::", "_"));
            let content = device::render(ir, device, name, opts)?;
            files.push((filename, content));
        }

        Ok(files)
    }
}
