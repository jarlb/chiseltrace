use std::{fs::{read_to_string, File}, io::BufReader};
use anyhow::Result;
use clap::{Parser, Subcommand};
use graphbuilder::GraphBuilder;
use pdg_spec::PDGSpec;
use errors::Error;

mod pdg_spec;
mod conversion;
mod slicing;
mod errors;
mod cfg;
mod graphbuilder;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Perform a slice operation.
    Slice {
        /// The path to the input PDG
        path: String,
        /// The statement that should be used for the program slicing.
        slice_criterion: String,
    },
    /// Perform a dynamic slice operation.
    DynSlice {
        /// The path to the input PDG
        pdg_path: String,
        /// The path the the VCD file
        vcd_path: String,
        /// The statement that should be used for the program slicing.
        slice_criterion: String,
    },
    /// Perform a conversion from FIRRTL PDG to CHISEL PDG operation.
    Convert {
        /// The path to the input PDG
        path: String,
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let argpath = match &args.command {
        Commands::Slice { path, .. } => path,
        Commands::Convert { path } => path,
        Commands::DynSlice { pdg_path, .. } => pdg_path
    };
    let buf = read_to_string(argpath)?;
    let pdg_raw = serde_json::from_str::<PDGSpec>(buf.as_str())?;

    match &args.command {
        Commands::Slice { slice_criterion, .. } => {
            let sliced = slicing::pdg_slice(pdg_raw, &slice_criterion)?;
            slicing::write_pdg(&sliced, "out_pdg.json")?;
        },
        Commands::Convert {..} => conversion::pdg_convert(pdg_raw)?,
        Commands::DynSlice { pdg_path, vcd_path, slice_criterion } => {
            let sliced = slicing::pdg_slice(pdg_raw, &slice_criterion)?;
            slicing::write_pdg(&sliced, "out_pdg.json")?;

            let mut builder = GraphBuilder::new(vcd_path, sliced)?;
            builder.process()?;
        }
    }

    // let mut builder = GraphBuilder::new("trace.vcd")?;
    // builder.read_init()?;
    // builder.read_rest()?;

    Ok(())
}