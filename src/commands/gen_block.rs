use crate::{
    commands::{get_generate_opts, GenerateShared, RustArch},
    generate::rust,
    ir::IR,
};

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::{collections::HashSet, fs};

/// Generate Rust code from a YAML register block
#[derive(Parser)]
pub struct GenBlock {
    /// Input YAML path
    #[clap(short, long)]
    pub input: PathBuf,
    /// Output Rust code path
    #[clap(short, long)]
    pub output: PathBuf,
    #[clap(flatten)]
    pub gen_shared: GenerateShared,
    /// Target CPU architecture (cortex-m or riscv).
    #[clap(long, value_enum, default_value = "cortex-m")]
    pub arch: RustArch,
}

pub fn gen_block(args: GenBlock) -> Result<()> {
    let data = fs::read(&args.input)?;
    let mut ir: IR = serde_yaml::from_slice(&data)?;

    let dependencies = ir
        .blocks
        .values()
        .filter_map(|block| block.extends.clone())
        .collect::<HashSet<_>>();

    for dependency in dependencies.iter() {
        let data = fs::read(args.input.with_file_name(dependency).with_extension("yaml"))?;
        let sub_ir: IR = serde_yaml::from_slice(&data)?;
        if dependencies.contains(sub_ir.blocks.keys().next().unwrap()) {
            ir.merge(sub_ir);
            break;
        }
    }

    crate::transform::expand_extends::ExpandExtends {}.run(&mut ir)?;
    crate::transform::sort::Sort {}.run(&mut ir)?;

    let generate_opts = get_generate_opts(args.gen_shared, args.arch)?;
    let items = rust::render(&ir, &generate_opts)?;
    fs::write(&args.output, items.to_string())?;

    Ok(())
}
