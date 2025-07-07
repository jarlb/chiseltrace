use std::{collections::HashSet, fs::{read_to_string, File}, io::BufWriter, path::Path};
use anyhow::Result;
use clap::{Parser, Subcommand};
use chiseltrace_rs::{conversion::{dpdg_make_exportable, pdg_convert_to_source}, graphbuilder::GraphProcessingType, slicing::{pdg_slice, write_dynamic_slice, write_static_slice}, util::parse_criterion};
use chiseltrace_rs::graphbuilder::{GraphBuilder, CriterionType};
use chiseltrace_rs::pdg_spec::PDGSpec;
use chiseltrace_rs::sim_data_injection::TywavesInterface;
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

        #[clap(default_value = "slice.json")]
        output_path: String,
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
        #[arg(
            value_parser = parse_criterion,
            help = "Criterion in format 'type:value' (e.g., 'statement:connect_io.a')"
        )]
        slice_criterion: CriterionType,
        /// Maximum amount of timesteps
        max_timesteps: Option<u64>,
        /// The name of the top-level module
        top_module: String,

        /// Specifies additional scopes that will be used while processing.
        #[clap(value_delimiter = ' ', num_args = 1..)]
        extra_scopes: Option<Vec<String>>,

        #[clap(default_value = "dynpdg.json")]
        output_path: String,
    },
    
    DynSlice {
        /// The path to the input PDG
        pdg_path: String,
        /// The path the the VCD file
        vcd_path: String,
        /// The statement that should be used for the program slicing.
        #[arg(
            value_parser = parse_criterion,
            help = "Criterion in format 'type:value' (e.g., 'statement:connect_io.a')"
        )]
        slice_criterion: CriterionType,
        /// Maximum amount of timesteps
        #[arg(long)]
        max_timesteps: Option<u64>,
        /// Specifies additional scopes that will be used while processing.
        #[clap(long, value_delimiter = ' ', num_args = 1..)]
        extra_scopes: Option<Vec<String>>,

        #[clap(long, default_value = "dynslice.json")]
        output_path: String,
    },
    /// Perform a conversion from FIRRTL PDG to Chisel PDG operation.
    Convert {
        /// The path to the input PDG
        path: String,
        #[clap(default_value = "converted_pdg.json")]
        output_path: String,
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let argpath = match &args.command {
        Commands::Slice { path, .. } => path,
        Commands::Convert { path , ..} => path,
        Commands::DynPDG { pdg_path, .. } => pdg_path,
        Commands::DynSlice { pdg_path, ..} => pdg_path
    };
    let buf = read_to_string(argpath)?;
    let mut deser = serde_json::Deserializer::from_str(buf.as_str());
    deser.disable_recursion_limit();
    let pdg_raw = PDGSpec::deserialize(&mut deser)?;

    match &args.command {
        Commands::Slice { slice_criterion, output_path, .. } => {
            let sliced = pdg_slice(pdg_raw, slice_criterion)?;
            let converted = pdg_convert_to_source(sliced.into(), true, false);
            write_static_slice(&converted, output_path)?;
        },
        Commands::Convert { output_path, .. } => {
            let converted = pdg_convert_to_source(pdg_raw.into(), true, false);
            let output_file = File::create(output_path)?;
            let writer = BufWriter::new(output_file);
        
            serde_json::to_writer_pretty(writer, &converted)?;
        },
        Commands::DynPDG { pdg_path:_, vcd_path, hgldd_path, slice_criterion, max_timesteps, top_module, extra_scopes, output_path } => {
            let max_timesteps = max_timesteps.map(|x| x as i64);
            // let sliced = pdg_slice(pdg_raw, slice_criterion)?;
            let sliced  = pdg_raw;
            // write_pdg(&sliced, "out_pdg.json")?;

            println!("Starting dynamic PDG building");
            let mut builder = GraphBuilder::new(vcd_path, extra_scopes.clone().unwrap_or(vec![]), sliced)?;
            let dpdg = builder.process(&slice_criterion, max_timesteps, GraphProcessingType::Normal)?;

            println!("Making DPDG exportable");
            let dpdg = dpdg_make_exportable(dpdg);
            
            println!("Converting to source representation");
            let mut converted_pdg = pdg_convert_to_source(dpdg, true, true);

            println!("Adding tywaves info");
            let tywaves = TywavesInterface::new(Path::new(hgldd_path),
                vec!["TOP".into(), "svsimTestbench".into(), "dut".into()], &top_module)?;
            
            let tywaves_vcd_path = tywaves.vcd_rewrite(Path::new(vcd_path))?;
            println!("VCD rewritten");
            tywaves.inject_sim_data(&mut converted_pdg, &tywaves_vcd_path)?;

            let mut lines = HashSet::new();
            for vert in &converted_pdg.vertices {
                if vert.timestamp >= 80 {
                    lines.insert((vert.file.clone(), vert.line));
                }
            }
            println!("Unique source lines in DPDG: {}", lines.len());
            println!("Num verts: {}, num edges: {}", converted_pdg.vertices.len(), converted_pdg.edges.len());
    
            let f = File::create(&output_path)?;
            let writer = BufWriter::new(f);
            serde_json::to_writer_pretty(writer, &converted_pdg)?;
        }
        Commands::DynSlice { pdg_path:_, vcd_path, slice_criterion, max_timesteps, extra_scopes, output_path } => {
            let sliced  = pdg_raw;
            let max_timesteps = max_timesteps.map(|x| x as i64);

            println!("Starting dynamic PDG building");
            let mut builder = GraphBuilder::new(vcd_path, extra_scopes.clone().unwrap_or(vec![]), sliced)?;
            let dpdg = builder.process(&slice_criterion, max_timesteps.clone(), GraphProcessingType::Full)?;

            write_dynamic_slice(&dpdg, output_path)?;
        }
    }

    Ok(())
}