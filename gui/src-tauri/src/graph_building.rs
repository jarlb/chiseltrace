use std::{collections::{HashMap, HashSet}, fs::{read_to_string, File}, io::BufReader, sync::RwLock};

use program_slicer_lib::{conversion::{dpdg_make_exportable, pdg_convert_to_source}, graphbuilder::GraphBuilder, pdg_spec::PDGSpec, sim_data_injection::TywavesInterface, slicing::pdg_slice};
use serde::Deserialize;
use tauri::State;
use anyhow::Result;

use crate::{app_state::{AppState, ViewableGraph}, errors::map_err_to_string_async};

#[tauri::command]
pub async fn make_dpdg(state: State<'_, RwLock<AppState>>) -> Result<(), String> {
    map_err_to_string_async(async {
        let pdg_config = {
            // Prevent global state lock during graph building.
            let state_guard = state.read().map_err(|_| anyhow::anyhow!("RwLock poisoned"))?;
            state_guard.pdg_config.clone()
        };

        let Some(pdg_config) = pdg_config else {
            anyhow::bail!("Tried building PDG before config was known.");
        };
        
        let reader = BufReader::new(File::open(&pdg_config.pdg_path)?);

        let mut deser = serde_json::Deserializer::from_reader(reader);
        deser.disable_recursion_limit();
        //serde_json::from_str::<PDGSpec>(buf.as_str())?;
        let pdg_raw = PDGSpec::deserialize(&mut deser)?;
        let sliced = pdg_raw;

        println!("Read PDG from file");

        // First do a static slice to try to reduce the amount of analyzed nodes
        // let sliced = pdg_slice(pdg_raw, &pdg_config.criterion)?;

        // Build the DPDG
        let mut builder = GraphBuilder::new(&pdg_config.vcd_path, pdg_config.extra_scopes.clone(), sliced)?;
        let dpdg = builder.process(&pdg_config.criterion, pdg_config.max_timesteps.map(|t| t as i64), pdg_config.data_only)?;

        println!("DPDG build complete");

        let dpdg = dpdg_make_exportable(dpdg);
        println!("Made DPDG exportable");

        // Convert to source language
        let mut converted_pdg = pdg_convert_to_source(dpdg, false);

        println!("Converted to source representation");

        // Add simulation data
        let tywaves = TywavesInterface::new(&pdg_config.hgldd_path, pdg_config.extra_scopes.clone(), &pdg_config.top_module)?;
    
        let tywaves_vcd_path = tywaves.vcd_rewrite(&pdg_config.vcd_path)?;
        println!("VCD rewrite done");
        tywaves.inject_sim_data(&mut converted_pdg, &tywaves_vcd_path)?;

        for v in &mut converted_pdg.vertices {
            v.timestamp += 1;
        }

        //let converted_pdg = dpdg;

        println!("Data injection done");

        // Create maps to speed up the viewer
        let mut time_to_nodes = HashMap::new();
        for (idx, v) in converted_pdg.vertices.iter().enumerate() {
            time_to_nodes.entry(v.timestamp).and_modify(|nodes: &mut Vec<usize>| nodes.push(idx)).or_insert(vec![idx]);
        }

        let mut dep_to_edges = HashMap::new();
        for (idx, e) in converted_pdg.edges.iter().enumerate() {
            dep_to_edges.entry(e.from).and_modify(|edges: &mut Vec<usize>| edges.push(idx)).or_insert(vec![idx]);
        }

        let mut prov_to_edges = HashMap::new();
        for (idx, e) in converted_pdg.edges.iter().enumerate() {
            prov_to_edges.entry(e.to).and_modify(|edges: &mut Vec<usize>| edges.push(idx)).or_insert(vec![idx]);
        }

        let n_timestamps = converted_pdg.vertices.iter().fold(0, |acc, x| acc.max(x.timestamp)) as u64;

        // Find unique source files
        let mut source_paths = HashSet::new();
        for v in &converted_pdg.vertices {
            source_paths.insert(v.file.clone());
        }

        // Just read them all to memory, then can't be that big
        let mut source_files = HashMap::new();
        for p in source_paths {
            // This is a hacky fix that appends the root symbol / if the file is in the home directory.
            // For some reason, the exported PDG does not contain this symbol.
            // This will not work on windows.
            let read_path = if p.starts_with("home") {
                &("/".to_string() + &p)
            } else {
                &p
            };

            if let Ok(contents) = read_to_string(&read_path) {
                source_files.insert(p, contents.lines().map(String::from).collect());
            }
        }

        let viewable_graph = ViewableGraph {
            dpdg: converted_pdg,
            time_to_nodes,
            dep_to_edges,
            prov_to_edges,
            n_timestamps,
            source_files
        };

        let mut state_guard = state.write().map_err(|_| anyhow::anyhow!("RwLock poisoned"))?;
        state_guard.graph = Some(viewable_graph);

        Ok(())
    }).await
}