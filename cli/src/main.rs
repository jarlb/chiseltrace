use std::{collections::HashSet, fs::{read_to_string, File}, io::BufWriter, path::Path};
use anyhow::Result;
use clap::{Parser, Subcommand};
use program_slicer_lib::{conversion::{dpdg_make_exportable, pdg_convert_to_source}, slicing::{pdg_slice, write_dynamic_slice, write_pdg, write_static_slice}};
use program_slicer_lib::graphbuilder::{GraphBuilder, CriterionType};
use program_slicer_lib::pdg_spec::PDGSpec;
use program_slicer_lib::sim_data_injection::TywavesInterface;
use serde::Deserialize;

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
    /// Convert to a dynamic program dependency graph.
    DynPDG {
        /// The path to the input PDG
        pdg_path: String,
        /// The path the the VCD file
        vcd_path: String,
        /// Path to the HGLDD directory
        hgldd_path: String,
        /// The statement that should be used for the program slicing.
        slice_criterion: String,
        /// Maximum amount of timesteps
        max_timesteps: Option<u64>,
    },
    
    DynSlice {
        /// The path to the input PDG
        pdg_path: String,
        /// The path the the VCD file
        vcd_path: String,
        /// The statement that should be used for the program slicing.
        slice_criterion: String,
    },
    /// Perform a conversion from FIRRTL PDG to Chisel PDG operation.
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
        Commands::DynPDG { pdg_path, .. } => pdg_path,
        Commands::DynSlice { pdg_path, ..} => pdg_path
    };
    let buf = read_to_string(argpath)?;
    let mut deser = serde_json::Deserializer::from_str(buf.as_str());
    deser.disable_recursion_limit();
    //serde_json::from_str::<PDGSpec>(buf.as_str())?;
    let pdg_raw = PDGSpec::deserialize(&mut deser)?;

    match &args.command {
        Commands::Slice { slice_criterion, .. } => {
            let sliced = pdg_slice(pdg_raw, slice_criterion)?;
            let converted = pdg_convert_to_source(sliced.into(), true, false);
            write_static_slice(&converted, "out_pdg.json")?;
        },
        Commands::Convert {..} => {
            let converted = pdg_convert_to_source(pdg_raw.into(), true, false);
            let output_file = File::create("out_chisel_pdg.json")?;
            let writer = BufWriter::new(output_file);
        
            serde_json::to_writer_pretty(writer, &converted)?;
        },
        Commands::DynPDG { pdg_path:_, vcd_path, hgldd_path, slice_criterion, max_timesteps } => {
            let max_timesteps = max_timesteps.map(|x| x as i64);
            // let sliced = pdg_slice(pdg_raw, slice_criterion)?;
            let sliced  = pdg_raw;
            // write_pdg(&sliced, "out_pdg.json")?;

            println!("Starting dynamic PDG building");
            let mut builder = GraphBuilder::new(vcd_path, vec!["TOP".into(), "svsimTestbench".into(), "dut".into()], sliced)?;
            let dpdg = builder.process(&CriterionType::Signal(slice_criterion.clone()), max_timesteps, false)?;

            println!("Making DPDG exportable");
            let dpdg = dpdg_make_exportable(dpdg);
            
            println!("Converting to source representation");
            let mut converted_pdg = pdg_convert_to_source(dpdg, true, true);

            println!("Adding tywaves info");
            let tywaves = TywavesInterface::new(Path::new(hgldd_path),
                vec!["TOP".into(), "svsimTestbench".into(), "dut".into()], &"Core".into())?;
            
            let tywaves_vcd_path = tywaves.vcd_rewrite(Path::new(vcd_path))?;
            println!("VCD rewritten");
            tywaves.inject_sim_data(&mut converted_pdg, &tywaves_vcd_path)?;
            // let signal = tywaves.find_signal(&["TOP".into(), "svsimTestbench".into(), "dut".into(), "regfile".into(), "pred_io_w_en".into()])?;
            // println!("Translated variable: {:#?}", tywaves.translate_variable(&signal, &"0".repeat(1))?);

            let mut lines = HashSet::new();
            for vert in &converted_pdg.vertices {
                if vert.timestamp >= 80 {
                    lines.insert((vert.file.clone(), vert.line));
                }
            }
            println!("Unique source lines in DPDG: {}", lines.len());
            println!("Num verts: {}, num edges: {}", converted_pdg.vertices.len(), converted_pdg.edges.len());
    
            let f = File::create("dynpdg.json")?;
            let writer = BufWriter::new(f);
            serde_json::to_writer_pretty(writer, &converted_pdg)?;

            // println!("{:#?}", signal.create_val_repr(raw_val_vcd, render_fn));
        }
        Commands::DynSlice { pdg_path, vcd_path, slice_criterion } => {
            let sliced  = pdg_raw;
            // write_pdg(&sliced, "out_pdg.json")?;

            println!("Starting dynamic PDG building");
            let mut builder = GraphBuilder::new(vcd_path, vec!["TOP".into(), "svsimTestbench".into(), "dut".into()], sliced)?;
            let dpdg = builder.process(&CriterionType::Statement(slice_criterion.clone()), None, false)?;

            write_dynamic_slice(&dpdg, "dynslice.json")?;
        }
    }

    // let mut builder = GraphBuilder::new("trace.vcd")?;
    // builder.read_init()?;
    // builder.read_rest()?;

    Ok(())
}