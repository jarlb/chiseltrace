use std::{collections::HashMap, fs::File, io::BufReader, sync::RwLock};

use program_slicer_lib::{conversion::pdg_convert_to_source, graphbuilder::GraphBuilder, sim_data_injection::TywavesInterface, slicing::pdg_slice};
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
        let pdg_raw = serde_json::from_reader(reader)?;
        let sliced = pdg_slice(pdg_raw, &pdg_config.criterion)?;

        let mut builder = GraphBuilder::new(&pdg_config.vcd_path, pdg_config.extra_scopes.clone(), sliced)?;
        let dpdg = builder.process()?;

        let mut converted_pdg = pdg_convert_to_source(dpdg, false);

        let tywaves = TywavesInterface::new(&pdg_config.hgldd_path, pdg_config.extra_scopes.clone(), &pdg_config.top_module)?;
    
        let tywaves_vcd_path = tywaves.vcd_rewrite(&pdg_config.vcd_path)?;
        tywaves.inject_sim_data(&mut converted_pdg, &tywaves_vcd_path)?;

        let mut time_to_nodes = HashMap::new();
        for (idx, v) in converted_pdg.vertices.iter().enumerate() {
            time_to_nodes.entry(v.timestamp).and_modify(|nodes: &mut Vec<usize>| nodes.push(idx)).or_insert(vec![idx]);
        }

        let mut dep_to_edges = HashMap::new();
        for (idx, e) in converted_pdg.edges.iter().enumerate() {
            dep_to_edges.entry(e.from).and_modify(|edges: &mut Vec<usize>| edges.push(idx)).or_insert(vec![idx]);
        }

        let n_timestamps =converted_pdg.vertices.iter().fold(0, |acc, x| acc.max(x.timestamp));

        let viewable_graph = ViewableGraph {
            dpdg: converted_pdg,
            time_to_nodes,
            dep_to_edges,
            n_timestamps
        };

        let mut state_guard = state.write().map_err(|_| anyhow::anyhow!("RwLock poisoned"))?;
        state_guard.graph = Some(viewable_graph);

        Ok(())
    }).await
}