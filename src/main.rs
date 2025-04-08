use std::{fs::{read_to_string, File}, io::{BufReader, BufWriter}, path::Path};
use anyhow::Result;
use clap::{Parser, Subcommand};
use conversion::pdg_convert_to_source;
use graphbuilder::GraphBuilder;
use pdg_spec::PDGSpec;
use errors::Error;
use sim_data_injection::TywavesInterface;
use tywaves_rs::tyvcd::trace_pointer::TraceGetter;

mod pdg_spec;
mod conversion;
mod slicing;
mod errors;
mod graphbuilder;
mod sim_data_injection;

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
        Commands::Convert {..} => {
            let converted = conversion::pdg_convert_to_source(pdg_raw.into());
            let output_file = File::create("out_chisel_pdg.json")?;
            let writer = BufWriter::new(output_file);
        
            serde_json::to_writer_pretty(writer, &converted)?;
        },
        Commands::DynSlice { pdg_path, vcd_path, slice_criterion } => {
            let sliced = slicing::pdg_slice(pdg_raw, &slice_criterion)?;
            slicing::write_pdg(&sliced, "out_pdg.json")?;

            let mut builder = GraphBuilder::new(vcd_path, sliced)?;
            let dpdg = builder.process()?;

            let mut converted_pdg = pdg_convert_to_source(dpdg);

            let tywaves = TywavesInterface::new(&Path::new("./resources/hgldd"),
                vec!["TOP".into(), "svsimTestbench".into(), "dut".into()], &"RegFileTester".into())?;
            
            let tywaves_vcd_path = tywaves.vcd_rewrite(&Path::new("./resources/trace.vcd"))?;
            tywaves.inject_sim_data(&mut converted_pdg, &tywaves_vcd_path)?;
            // let signal = tywaves.find_signal(&["TOP".into(), "svsimTestbench".into(), "dut".into(), "regfile".into(), "pred_io_w_en".into()])?;
            // println!("Translated variable: {:#?}", tywaves.translate_variable(&signal, &"0".repeat(1))?);

            println!("Num verts: {}, num edges: {}", converted_pdg.vertices.len(), converted_pdg.edges.len());
    
            let f = File::create("dynpdg.json")?;
            let writer = BufWriter::new(f);
            serde_json::to_writer_pretty(writer, &converted_pdg)?;

            // println!("{:#?}", signal.create_val_repr(raw_val_vcd, render_fn));
        }
    }

    // let mut builder = GraphBuilder::new("trace.vcd")?;
    // builder.read_init()?;
    // builder.read_rest()?;

    Ok(())
}