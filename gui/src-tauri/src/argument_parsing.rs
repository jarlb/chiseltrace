use std::path::Path;

use clap::Parser;
use anyhow::Result;
use program_slicer_lib::{graphbuilder::CriterionType, util::parse_criterion};

use crate::errors;

/// A GUI program to visualize chisel dynamic program dependency graphs
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Slicing criterion (e.g. the statement that will be backtraced)
    #[arg(
        short,
        long,
        value_parser = parse_criterion,
        help = "Criterion in format 'type:value' (e.g., 'statement:connect_io.a')"
    )]
    pub slice_criterion: CriterionType,

    /// Path to the program dependency graph exported by chisel
    #[arg(short, long)]
    pub pdg_path: String,

    /// Path to the VCD file
    #[arg(short, long)]
    pub vcd_path: String,

    /// Path to the HGLDD directory
    #[arg(long)]
    pub hgldd_path: String,

    /// The name of the top-level module
    #[arg(short, long)]
    pub top_module: String,

    /// Specifies additional scopes that will be used while processing.
    #[clap(short, long, value_delimiter = ' ', num_args = 1..)]
    pub extra_scopes: Option<Vec<String>>,

    #[arg(long)]
    pub max_timesteps: Option<u64>,

    #[arg(long)]
    pub data_only: Option<bool>
}

impl Args {
    pub fn validate(self) -> Result<Self> {
        let pdg_path = Path::new(&self.pdg_path);
        if !(pdg_path.exists() && pdg_path.is_file()) {
            anyhow::bail!(errors::Error::ArgumentValidationError("Invalid PDG path".into()));
        }

        let vcd_path = Path::new(&self.vcd_path);
        if !(vcd_path.exists() && vcd_path.is_file()) {
            anyhow::bail!(errors::Error::ArgumentValidationError("Invalid VCD path".into()));
        }

        let hgldd_path = Path::new(&self.hgldd_path);
        if !(hgldd_path.exists() && hgldd_path.is_dir()) {
            anyhow::bail!(errors::Error::ArgumentValidationError("Invalid HGLDD path".into()));
        }

        Ok(self)
    }
}