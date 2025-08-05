use std::{collections::HashSet, process::Command, sync::RwLock};

use anyhow::anyhow;
use itertools::Itertools;
use chiseltrace_rs::pdg_spec::{ExportablePDG, PDGSpecEdgeKind, PDGSpecNodeKind};
use serde::Serialize;
use tauri::State;

use crate::{app_state::{AppState, ViewableGraph}, errors::map_err_to_string, graph_building::rebuild_hier_graph, translation::{interpret_tywaves_value, TranslationStrategy}};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewerGraph {
    vertices: Vec<ViewerNode>,
    edges: Vec<ViewerEdge>
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewerNode {
    id: u64,
    label: String,
    group: String, // The timeslot group
    module_path: Vec<String>,
    timestamp: u64,
    long_distance: bool,
    color: NodeColour,
    shape: NodeShape,
    code: Option<String>,
    incoming: Vec<ViewerSignal>,
    outgoing: Vec<ViewerSignal>,
    file: String,
    line: u32
}

#[derive(Debug, Clone, Serialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ViewerSignal {
    name: String,
    value: String,
    connection_type: String
}

#[derive(Debug)]
enum NodeColour {
    Yellow,
    Green,
    Blue,
    Red
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum NodeShape {
    Ellipse,
    Box,
    Diamond
}

impl From<PDGSpecNodeKind> for NodeColour {
    fn from(value: PDGSpecNodeKind) -> Self {
        match value {
            PDGSpecNodeKind::Connection => NodeColour::Blue,
            PDGSpecNodeKind::ControlFlow => NodeColour::Red,
            PDGSpecNodeKind::IO => NodeColour::Green,
            PDGSpecNodeKind::DataDefinition => NodeColour::Blue,
            PDGSpecNodeKind::Definition => NodeColour::Yellow
        }
    }
}

impl NodeColour {
    fn to_hex(&self) -> String {
        match self {
            &NodeColour::Blue => "#97C2FC",
            &NodeColour::Red => "#FB7E81",
            &NodeColour::Green => "#7BE141",
            &NodeColour::Yellow => "#FFFF00"
        }.into()
    }
}


impl Serialize for NodeColour {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl From<PDGSpecNodeKind> for NodeShape {
    fn from(value: PDGSpecNodeKind) -> Self {
        match value {
            PDGSpecNodeKind::Connection => NodeShape::Ellipse,
            PDGSpecNodeKind::ControlFlow => NodeShape::Diamond,
            _ => NodeShape::Box
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewerEdge {
    from: u64,
    to: u64,
    arrows: String,
    color: EdgeColour,
    dotted: bool,
    label: String
    // Same here, add colours, simulation values etc.
}

#[derive(Debug)]
enum EdgeColour {
    Blue,
    Red,
    Purple
}

impl From<PDGSpecEdgeKind> for EdgeColour {
    fn from(value: PDGSpecEdgeKind) -> Self {
        match value {
            PDGSpecEdgeKind::Data => EdgeColour::Blue,
            PDGSpecEdgeKind::Conditional => EdgeColour::Red,
            PDGSpecEdgeKind::Index => EdgeColour::Purple,
            PDGSpecEdgeKind::Declaration => EdgeColour::Blue,
        }
    }
}

impl EdgeColour {
    fn to_hex(&self) -> String {
        match self {
            &EdgeColour::Blue => "#97C2FC".into(),
            &EdgeColour::Red => "#FB7E81".into(),
            &EdgeColour::Purple => "#bc2dcc".into(),
        }
    }
}


impl Serialize for EdgeColour {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

/// Get the signals that will be displayed in the hover tooltip
fn get_viewer_signals(dpdg: &ExportablePDG, edges: &Vec<usize>, incoming: bool) -> Vec<ViewerSignal> {
    edges.iter().map(|e| {
        let edge = &dpdg.edges[*e];
        let destination =  if incoming {
            &dpdg.vertices[edge.to as usize]
        } else {
            &dpdg.vertices[edge.from as usize]
        };
        let name = if let Some(signal) = &destination.related_signal {
            if signal.field_path.is_empty() {
                signal.signal_path.clone()
            } else {
                format!("{} [{}]", signal.signal_path, signal.field_path)
            }
        } else { "".into() };
        let value = destination.sim_data.as_ref().map(|d|  {
            let translated = interpret_tywaves_value(&d, TranslationStrategy::Auto);
            // format!("{} {}", translated.tpe.unwrap_or("".into()), translated.value)
            translated.value
        }).unwrap_or("".into());
        let connection_type = match edge.kind {
            PDGSpecEdgeKind::Conditional => "controlflow",
            PDGSpecEdgeKind::Data => "data",
            PDGSpecEdgeKind::Index => "index",
            PDGSpecEdgeKind::Declaration => ""
        }.into();
        ViewerSignal {
            name,
            value,
            connection_type
        }
    }).unique().collect()
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

/// A command that toggles the expanded state of a group of nodes
#[tauri::command]
pub fn toggle_module(state: State<'_, RwLock<AppState>>, module_path: Vec<String>, timestamp: i64) -> Result<(), String> {
    map_err_to_string(|| {
        let mut should_rebuild_graph = false;
        {
            let mut state_guard = state.write().map_err(|_| anyhow!("RwLock poisoned"))?;
            let Some(graph) = &mut state_guard.graph else {
                anyhow::bail!("Uninitialized graph!");
            };

            if let Some(hier) = &mut graph.node_hierarchy {
                if module_path.len() > 0 && module_path[0].len() > 0 {
                    let mut group = hier[timestamp as usize].clone();
                    for path_part in &module_path {
                        let new_group = {
                            let group_lock = group.read().unwrap();
                            let Some(new_group) = group_lock.children.iter().find(|g| g.read().unwrap().instance_name.eq(path_part)) else {
                                return Ok(());
                            };
                            new_group.clone()
                        };
                        group = new_group
                    }
                    // Now we have the module we want to toggle
                    let mut guard = group.write().unwrap();
                    guard.expanded = !guard.expanded;
                    should_rebuild_graph = true;
                }
            }
        }
        if should_rebuild_graph {
            rebuild_hier_graph(&state)?;
        }
        Ok(())
    })
}

/// Sets the new graph head by calculating reachability and setting other nodes to hidden
#[tauri::command]
pub fn set_new_head(state: State<'_, RwLock<AppState>>, id: usize) -> Result<(), String> {
    map_err_to_string(|| {
        let mut state_guard = state.write().map_err(|_| anyhow!("RwLock poisoned"))?;
        let Some(graph) = &mut state_guard.graph else {
            anyhow::bail!("Uninitialized graph!");
        };

        // Some IDs may not correspond to a real node: ignore those
        if id >= graph.dpdg.vertices.len() {
            return Ok(());
        }

        let mut nodes_reached = HashSet::new();

        let mut stack = vec![id];
        while let Some(node_idx) = stack.pop() {
            nodes_reached.insert(node_idx);
            if let Some(edges) = graph.dep_to_edges.get(&(node_idx as u32)) {
                for edge_idx in edges {
                    let edge = &graph.dpdg.edges[*edge_idx];
                    if !nodes_reached.contains(&(edge.to as usize)) {
                        stack.push(edge.to as usize);
                    }
                }
            }
        }
        
        graph.shown_ids = nodes_reached;

        Ok(())
    })
}

/// Resets the graph head
#[tauri::command]
pub fn reset_head(state: State<'_, RwLock<AppState>>) -> Result<(), String> {
    map_err_to_string(|| {
        let mut state_guard = state.write().map_err(|_| anyhow!("RwLock poisoned"))?;
        let Some(graph) = &mut state_guard.graph else {
            anyhow::bail!("Uninitialized graph!");
        };
        
        graph.shown_ids = (0..graph.dpdg.vertices.len()).collect();

        Ok(())
    })
}

#[tauri::command]
pub fn open_vs_code(state: State<'_, RwLock<AppState>>, id: usize) -> Result<(), String> {
    map_err_to_string(|| {
        let mut state_guard = state.write().map_err(|_| anyhow!("RwLock poisoned"))?;
        let Some(graph) = &mut state_guard.graph else {
            anyhow::bail!("Uninitialized graph!");
        };
        
        if id >= graph.dpdg.vertices.len() {
            return Ok(());
        }

        let node = &graph.dpdg.vertices[id as usize];
        Command::new("code")
            .arg("--goto")
            .arg(format!("{}:{}", node.file, node.line))
            .spawn()?;

        Ok(())
    })
}

/// Retrieves a part of the complete dpdg between a start and end timestamp
#[tauri::command]
pub fn get_partial_graph(state: State<'_, RwLock<AppState>>, range_begin: u64, range_end: u64) -> Result<String, String> {
    map_err_to_string(|| {
        let state_guard = state.read().map_err(|_| anyhow!("RwLock poisoned"))?;
        let Some(graph) = &state_guard.graph else {
            anyhow::bail!("Uninitialized graph!");
        };

        if !graph.should_group_nodes { // Regular 
            let mut viewer_graph = ViewerGraph { vertices: vec![], edges: vec![] };

            for timestamp in range_begin..=range_end {
                let default_vec = vec![];
                let node_indices = graph.time_to_nodes.get(&(timestamp as i64)).unwrap_or(&default_vec);
                for idx in node_indices {
                    if !graph.shown_ids.contains(idx) {
                        continue;
                    }
                    let node = &graph.dpdg.vertices[*idx];
                    let edges = graph.dep_to_edges.get(&(*idx as u32));
                    let group = format!("t{}", graph.n_timestamps - timestamp);
                    let incoming = edges.map_or(vec![], |edges| get_viewer_signals(&graph.dpdg, edges, true));
                    let outgoing = graph.prov_to_edges.get(&(*idx as u32)).map_or(vec![], |edges| get_viewer_signals(&graph.dpdg, edges, true));
                    viewer_graph.vertices.push(ViewerNode {
                        id: *idx as u64,
                        label: node.name.clone(),
                        group: group.clone(),
                        module_path: node.module_path.clone(),
                        timestamp,
                        long_distance: false,
                        color: NodeColour::from(node.kind),
                        shape: NodeShape::from(node.kind),
                        code: graph.source_files.get(&node.file).map(|v| v.get(node.line as usize - 1).map(|l| l.clone())).flatten(),
                        incoming,
                        outgoing,
                        file: node.file.clone(),
                        line: node.line
                    });
                    if let Some(edges) = edges {
                        for edge in edges {
                            let edge = &graph.dpdg.edges[*edge];
                            let destination = &graph.dpdg.vertices[edge.to as usize];
                            let label = if let Some(d) = &destination.sim_data {
                                let translated = interpret_tywaves_value(d, TranslationStrategy::Auto);
                                format!("{} {}", translated.tpe.unwrap_or("".into()), translated.value)
                            } else { "".into() };
                            if node.timestamp.abs_diff(destination.timestamp) > 3 {
                                let edges = graph.dep_to_edges.get(&edge.to);
                                let incoming = edges.map_or(vec![], |edges| get_viewer_signals(&graph.dpdg, edges, true));
                                let outgoing = graph.prov_to_edges.get(&edge.to).map_or(vec![], |edges| get_viewer_signals(&graph.dpdg, edges, true));
                                // If an edge goes to a node that is more than 3 timesteps away, instead add it as a long distance relation
                                // It is important to generate a unique ID for these pseudo-nodes, because they MUST be unique in the graph
                                viewer_graph.vertices.push(ViewerNode {
                                    id: edge.to as u64 + graph.dpdg.vertices.len() as u64 + edge.from as u64,
                                    label: destination.name.clone(),
                                    group: group.clone(),
                                    module_path: destination.module_path.clone(),
                                    timestamp,
                                    long_distance: true,
                                    color: NodeColour::from(destination.kind),
                                    shape: NodeShape::from(destination.kind),
                                    code: graph.source_files.get(&destination.file).map(|v| v.get(destination.line as usize - 1).map(|l| l.clone())).flatten(),
                                    incoming,
                                    outgoing,
                                    file: node.file.clone(),
                                    line: node.line
                                });
                                viewer_graph.edges.push(ViewerEdge {
                                    from: edge.from as u64,
                                    to: edge.to as u64 + graph.dpdg.vertices.len() as u64 + edge.from as u64,
                                    arrows: "to".into(),
                                    color: EdgeColour::from(edge.kind),
                                    dotted: edge.clocked,
                                    label
                                });
                            } else {
                                viewer_graph.edges.push(ViewerEdge {
                                    from: edge.from as u64,
                                    to: edge.to as u64,
                                    arrows: "to".into(),
                                    color: EdgeColour::from(edge.kind),
                                    dotted: edge.clocked,
                                    label
                                });
                            }
                        }
                    }
                }
            }
            Ok(serde_json::to_string(&viewer_graph)?)
        } else {
            // We are displaying grouped nodes. TODO: find a better solution without copying the entire thing
            let mut viewer_graph = ViewerGraph { vertices: vec![], edges: vec![] };
            let Some(hier_graph) = &graph.current_hier_dpdg else {
                anyhow::bail!("Hierarchical graph not initialized!");
            };

            for timestamp in range_begin..=range_end {
                let default_vec = vec![];
                let node_indices = hier_graph.time_to_nodes.get(&(timestamp as i64)).unwrap_or(&default_vec);
                for idx in node_indices {
                    let node = &hier_graph.dpdg.vertices[*idx];
                    if !graph.shown_ids.contains(&hier_graph.original_ids[*idx]) && node.kind != PDGSpecNodeKind::Definition {
                        continue;
                    }
                    if let Some(hier_group) = hier_graph.group_ids.get(idx) {
                        let guard = hier_group.read().unwrap();
                        let group_ids = &guard.node_indices;
                        let mut show_group = false;
                        for id in group_ids {
                            if graph.shown_ids.contains(id) {
                                show_group = true;
                                break;
                            }
                        }
                        if !show_group {
                            continue;
                        }
                    }
                    let edges = hier_graph.dep_to_edges.get(&(*idx as u32));
                    let group = format!("t{}", graph.n_timestamps - timestamp);
                    let incoming = edges.map_or(vec![], |edges| get_viewer_signals(&hier_graph.dpdg, edges, true));
                    let outgoing = hier_graph.prov_to_edges.get(&(*idx as u32)).map_or(vec![], |edges| get_viewer_signals(&hier_graph.dpdg, edges, true));
                    viewer_graph.vertices.push(ViewerNode {
                        id: hier_graph.original_ids[*idx] as u64,
                        label: node.name.clone(),
                        group: group.clone(),
                        module_path: node.module_path.clone(),
                        timestamp,
                        long_distance: false,
                        color: NodeColour::from(node.kind),
                        shape: NodeShape::from(node.kind),
                        code: graph.source_files.get(&node.file).map(|v| v.get(node.line as usize - 1).map(|l| l.clone())).flatten(),
                        incoming,
                        outgoing,
                        file: node.file.clone(),
                        line: node.line
                    });
                    if let Some(edges) = edges {
                        for edge in edges {
                            let edge = &hier_graph.dpdg.edges[*edge];
                            let destination = &hier_graph.dpdg.vertices[edge.to as usize];
                            let label = if let Some(d) = &destination.sim_data {
                                let translated = interpret_tywaves_value(d, TranslationStrategy::Auto);
                                format!("{} {}", translated.tpe.unwrap_or("".into()), translated.value)
                            } else { "".into() };
                            if node.timestamp.abs_diff(destination.timestamp) > 3 {
                                let edges = hier_graph.dep_to_edges.get(&edge.to);
                                let incoming = edges.map_or(vec![], |edges| get_viewer_signals(&hier_graph.dpdg, edges, true));
                                let outgoing = hier_graph.prov_to_edges.get(&edge.to).map_or(vec![], |edges| get_viewer_signals(&hier_graph.dpdg, edges, true));
                                // If an edge goes to a node that is more than 3 timesteps away, instead add it as a long distance relation
                                // It is important to generate a unique ID for these pseudo-nodes, because they MUST be unique in the graph
                                let node_id = (hier_graph.original_ids[edge.to as usize] << 32) as u64 + 10 * graph.dpdg.vertices.len() as u64 + hier_graph.original_ids[edge.from as usize] as u64;
                                viewer_graph.vertices.push(ViewerNode {
                                    // TODO: replace the 10x with an actual fix. This just shifts the duplicate ID problem elsewhere.
                                    id: node_id,
                                    label: destination.name.clone(),
                                    group: group.clone(),
                                    module_path: destination.module_path.clone(),
                                    timestamp,
                                    long_distance: true,
                                    color: NodeColour::from(destination.kind),
                                    shape: NodeShape::from(destination.kind),
                                    code: graph.source_files.get(&destination.file).map(|v| v.get(destination.line as usize - 1).map(|l| l.clone())).flatten(),
                                    incoming,
                                    outgoing,
                                    file: node.file.clone(),
                                    line: node.line
                                });
                                viewer_graph.edges.push(ViewerEdge {
                                    from: hier_graph.original_ids[edge.from as usize] as u64,
                                    to:  node_id,
                                    arrows: "to".into(),
                                    color: EdgeColour::from(edge.kind),
                                    dotted: edge.clocked,
                                    label
                                });
                            } else {
                                viewer_graph.edges.push(ViewerEdge {
                                    from: hier_graph.original_ids[edge.from as usize] as u64,
                                    to: hier_graph.original_ids[edge.to as usize] as u64,
                                    arrows: "to".into(),
                                    color: EdgeColour::from(edge.kind),
                                    dotted: edge.clocked,
                                    label
                                });
                            }
                        }
                    }
                }
            }
            Ok(serde_json::to_string(&viewer_graph)?)
        }
    })
}
