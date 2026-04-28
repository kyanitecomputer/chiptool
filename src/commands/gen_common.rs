use crate::generate::rust;
use anyhow::Result;
use clap::Parser;
use std::{fs, path::PathBuf};

/// Output the common.rs file
#[derive(Parser)]
pub struct GenCommon {
    /// Output Rust code *file* path
    #[clap(short, long)]
    pub output: PathBuf,
}

pub fn gen_common(args: GenCommon) -> Result<()> {
    fs::write(&args.output, rust::COMMON_MODULE)?;

    Ok(())
}
