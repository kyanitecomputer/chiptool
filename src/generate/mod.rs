pub mod rust;
pub mod c;     // Phase 3: C generator
pub mod go;     // Phase 4: Go generator

use anyhow::Result;
use crate::ir::IR;

/// Common trait for all code generators.
pub trait CodeGenerator {
    /// Output type: TokenStream for Rust, String for C/Go
    type Output;
    /// Language-specific options
    type Options;

    /// Generate code from IR, returning (filename, content) pairs
    fn generate(&self, ir: &IR, opts: &Self::Options) -> Result<Vec<(String, Self::Output)>>;
}

// Re-export Rust items for backward compatibility
pub use rust::{Options, DefmtOption, CommonModule, RustArch, render_device_x};
