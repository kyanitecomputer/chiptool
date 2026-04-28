use crate::commands::{
    apply_transform, clean_up_ir, get_generate_opts, load_svd, GenerateShared, NamespaceMode,
    RustArch,
};
use crate::{generate, svd2ir};
use crate::generate::CodeGenerator;
use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use std::fs;
use std::fs::File;
use std::path::PathBuf;

/// Supported output languages
#[derive(Clone, Debug, ValueEnum)]
pub enum Language {
    Rust,
    C,
    Go,
}

/// Generate a PAC from a SVD or IR YAML file.
#[derive(Parser)]
pub struct Generate {
    /// SVD file path (mutually exclusive with --yaml).
    #[clap(long, conflicts_with = "yaml")]
    pub svd: Option<PathBuf>,

    /// Chiptool IR YAML file path (mutually exclusive with --svd).
    /// Loads the IR directly without SVD conversion; useful with
    /// hand-written register YAML files.
    #[clap(long, conflicts_with = "svd")]
    pub yaml: Option<PathBuf>,

    /// Transforms file paths.
    #[clap(long)]
    pub transform: Vec<PathBuf>,
    /// Namespaces added to each extracted peripheral.
    #[clap(long, value_enum, default_value = "block-with-regs-vals")]
    pub namespaces: NamespaceMode,
    #[clap(flatten)]
    pub gen_shared: GenerateShared,

    /// Output language (rust, c, go)
    #[clap(short, long, value_enum, default_value = "rust")]
    pub language: Language,

    /// Target CPU architecture for Rust output (cortex-m or riscv).
    /// Only meaningful when --language rust is selected.
    #[clap(long, value_enum, default_value = "cortex-m")]
    pub arch: RustArch,

    /// Go package name for generated files.
    /// Only meaningful when --language go is selected.
    #[clap(long, default_value = "pac")]
    pub package: String,

    /// Go import path for the register-access package.
    /// Only meaningful when --language go is selected.
    /// Example: github.com/usbarmory/tamago/internal/reg
    #[clap(long, default_value = "tamago/reg")]
    pub reg_import: String,

    /// Output YAML path for the whole IR. Useful for debugging
    #[clap(long)]
    pub debug_ir_output: Option<PathBuf>,
    /// Output directory of the PAC files.
    #[clap(long)]
    pub output: Option<PathBuf>,
}

pub fn generate(args: Generate) -> Result<()> {
    let mut ir = match (&args.svd, &args.yaml) {
        (Some(svd_path), None) => {
            let svd = load_svd(svd_path)
                .with_context(|| format!("loading svd at {}", svd_path.display()))?;
            let ir = svd2ir::convert_svd(&svd, args.namespaces)
                .with_context(|| format!("converting svd at {}", svd_path.display()))?;
            ir
        }
        (None, Some(yaml_path)) => {
            let data = fs::read(yaml_path)
                .with_context(|| format!("reading yaml at {}", yaml_path.display()))?;
            let ir: crate::ir::IR = serde_yaml::from_slice(&data)
                .with_context(|| format!("parsing yaml at {}", yaml_path.display()))?;
            ir
        }
        (None, None) => bail!("one of --svd or --yaml is required"),
        (Some(_), Some(_)) => bail!("--svd and --yaml are mutually exclusive"),
    };

    clean_up_ir(&mut ir)?;

    for transform in args.transform {
        apply_transform(&mut ir, transform)?;
    }

    if let Some(path) = args.debug_ir_output {
        let f = File::create(&path)
            .with_context(|| format!("creating IR output yaml at {}", path.display()))?;
        serde_yaml::to_writer(f, &ir)
            .with_context(|| format!("writing IR output yaml at {}", path.display()))?;
    }

    let output_dir = if let Some(output) = args.output {
        output
    } else {
        std::env::current_dir()?
    };

    match args.language {
        Language::Rust => {
            let generate_opts = get_generate_opts(args.gen_shared, args.arch)?;
            let items: Vec<(String, proc_macro2::TokenStream)> =
                generate::rust::RustGenerator.generate(&ir, &generate_opts)?;
            for (filename, content) in items {
                let path = output_dir.join(filename);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, content.to_string())?;
            }

            // Rust-specific: generate device.x linker fragment (empty for RISC-V)
            let device_x = generate::render_device_x(
                &ir,
                ir.devices
                    .values()
                    .next()
                    .ok_or_else(|| anyhow!("Expected at least one device to be defined"))?,
                generate_opts.arch(),
            )?;
            if !device_x.is_empty() {
                fs::write(output_dir.join("device.x"), device_x)?;
            }
        }
        Language::C => {
            let opts = generate::c::COptions::default();
            let items = generate::c::CGenerator.generate(&ir, &opts)?;
            for (filename, content) in items {
                let path = output_dir.join(filename);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, content)?;
            }
        }
        Language::Go => {
            let opts = generate::go::GoOptions {
                package_name: args.package,
                reg_import: args.reg_import,
            };
            let items = generate::go::GoGenerator.generate(&ir, &opts)?;
            for (filename, content) in items {
                let path = output_dir.join(filename);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, content)?;
            }
        }
    }

    Ok(())
}
