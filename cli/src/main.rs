use std::{fs::{read_to_string, File}, io::BufWriter, path::Path};
use anyhow::Result;
use clap::{Parser, Subcommand};
use program_slicer_lib::{conversion::pdg_convert_to_source, slicing::{pdg_slice, write_pdg}};
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
        Commands::DynPDG { pdg_path, .. } => pdg_path
    };
    let buf = read_to_string(argpath)?;
    let mut deser = serde_json::Deserializer::from_str(buf.as_str());
    deser.disable_recursion_limit();
    //serde_json::from_str::<PDGSpec>(buf.as_str())?;
    let pdg_raw = PDGSpec::deserialize(&mut deser)?;

    match &args.command {
        Commands::Slice { slice_criterion, .. } => {
            let sliced = pdg_slice(pdg_raw, slice_criterion)?;
            write_pdg(&sliced, "out_pdg.json")?;
        },
        Commands::Convert {..} => {
            let converted = pdg_convert_to_source(pdg_raw.into(), true);
            let output_file = File::create("out_chisel_pdg.json")?;
            let writer = BufWriter::new(output_file);
        
            serde_json::to_writer_pretty(writer, &converted)?;
        },
        Commands::DynPDG { pdg_path:_, vcd_path, slice_criterion } => {
            // let sliced = pdg_slice(pdg_raw, slice_criterion)?;
            let sliced  = pdg_raw;
            // write_pdg(&sliced, "out_pdg.json")?;

            println!("Starting dynamic PDG building");
            let mut builder = GraphBuilder::new(vcd_path, vec!["TOP".into(), "svsimTestbench".into(), "dut".into()], sliced)?;
            let dpdg = builder.process(&CriterionType::Statement(slice_criterion.clone()), None)?;
            
            println!("Converting to source representation");
            let mut converted_pdg = pdg_convert_to_source(dpdg, true);

            println!("Adding tywaves info");
            let tywaves = TywavesInterface::new(Path::new("../resources/chiselwatt/hgldd"),
                vec!["TOP".into(), "svsimTestbench".into(), "dut".into()], &"Core".into())?;
            
            let tywaves_vcd_path = tywaves.vcd_rewrite(Path::new("../resources/chiselwatt/trace.vcd"))?;
            println!("VCD rewritten");
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