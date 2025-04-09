use std::sync::RwLock;

use anyhow::anyhow;
use program_slicer_lib::pdg_spec::{PDGSpecEdgeKind, PDGSpecNodeKind};
use serde::Serialize;
use tauri::State;

use crate::{app_state::AppState, errors::map_err_to_string, translation::{interpret_tywaves_value, TranslationStrategy}};

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
    timestamp: u64,
    long_distance: bool,
    color: NodeColour,
    shape: NodeShape
    // To add in the future: colours, shapes, hover information, click actions, etc.
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
            &NodeColour::Blue => "#97C2FC".into(),
            &NodeColour::Red => "#FB7E81".into(),
            &NodeColour::Green => "#7BE141".into(),
            &NodeColour::Yellow => "#FFFF00".into()
        }
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
        let mut viewer_graph = ViewerGraph { vertices: vec![], edges: vec![] };

        for timestamp in range_begin..=range_end {
            let node_indices = &graph.time_to_nodes[&timestamp];
            for idx in node_indices {
                let node = &graph.dpdg.vertices[*idx];
                let edges = graph.dep_to_edges.get(&(*idx as u32));
                let group = format!("t{}", graph.n_timestamps - timestamp);
                viewer_graph.vertices.push(ViewerNode {
                    id: *idx as u64,
                    label: node.name.clone(),
                    group: group.clone(),
                    timestamp,
                    long_distance: false,
                    color: NodeColour::from(node.kind),
                    shape: NodeShape::from(node.kind)
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
                            // If an edge goes to a node that is more than 3 timesteps away, instead add it as a long distance relation
                            // It is important to generate a unique ID for these pseudo-nodes, because they MUST be unique in the graph
                            viewer_graph.vertices.push(ViewerNode {
                                id: edge.to as u64 + graph.dpdg.vertices.len() as u64 + edge.from as u64,
                                label: destination.name.clone(),
                                group: group.clone(),
                                timestamp,
                                long_distance: true,
                                color: NodeColour::from(destination.kind),
                                shape: NodeShape::from(destination.kind)
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
    })
}