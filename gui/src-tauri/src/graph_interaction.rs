use std::sync::RwLock;

use anyhow::anyhow;
use serde::Serialize;
use tauri::State;

use crate::{app_state::AppState, errors::map_err_to_string};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewerGraph {
    vertices: Vec<ViewerNode>,
    long_distance_verts: Vec<ViewerNode>,
    edges: Vec<ViewerEdge>
}

#[derive(Debug, Serialize)]
struct ViewerNode {
    id: u64,
    label: String,
    group: String, // The timeslot group
    timestamp: u64
    // To add in the future: colours, shapes, hover information, click actions, etc.
}

#[derive(Debug, Serialize)]
struct ViewerEdge {
    from: u64,
    to: u64,
    arrows: String
    // Same here, add colours, simulation values etc.
}

#[tauri::command]
pub fn get_n_timeslots(state: State<'_, RwLock<AppState>>) -> Result<u64, String> {
    map_err_to_string(|| {
        let state_guard = state.read().map_err(|_| anyhow!("RwLock poisoned"))?;
        let Some(graph) = &state_guard.graph else {
            anyhow::bail!("Uninitialized graph!");
        };
        Ok(graph.n_timestamps)
    })
}

#[tauri::command]
pub fn get_partial_graph(state: State<'_, RwLock<AppState>>, range_begin: u64, range_end: u64) -> Result<String, String> {
    map_err_to_string(|| {
        let state_guard = state.read().map_err(|_| anyhow!("RwLock poisoned"))?;
        let Some(graph) = &state_guard.graph else {
            anyhow::bail!("Uninitialized graph!");
        };
        let mut viewer_graph = ViewerGraph { vertices: vec![], long_distance_verts: vec![], edges: vec![] };
        let mut node_ids_in_view = vec![];

        // Find the indices of all the nodes that are currently in view
        for timestamp in range_begin..=range_end {
            let node_indices = &graph.time_to_nodes[&timestamp];
            node_ids_in_view.extend_from_slice(node_indices);
        }
        for timestamp in range_begin..=range_end {
            let node_indices = &graph.time_to_nodes[&timestamp];
            for idx in node_indices {
                let node = &graph.dpdg.vertices[*idx];
                let edges = graph.dep_to_edges.get(&(*idx as u32));
                let group = format!("t{}", graph.n_timestamps - timestamp);
                viewer_graph.vertices.push(ViewerNode { id: *idx as u64, label: node.name.clone(), group, timestamp });
                if let Some(edges) = edges {
                for edge in edges {
                    let edge = &graph.dpdg.edges[*edge];
                    if node_ids_in_view.contains(&(edge.to as usize)) {
                        // The edge is between two nodes that are in view, add it
                        viewer_graph.edges.push(ViewerEdge { from: edge.from as u64, to: edge.to as u64, arrows: "to".into() });
                    }
                    }
                }
            }
        }
        Ok(serde_json::to_string(&viewer_graph)?)
    })
}