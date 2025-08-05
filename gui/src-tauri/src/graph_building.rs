use std::{collections::{HashMap, HashSet}, fs::{read_to_string, File}, io::BufReader, sync::{Arc, RwLock}, time::SystemTime};

use chiseltrace_rs::{conversion::{dpdg_make_exportable, pdg_convert_to_source}, graphbuilder::{GraphBuilder, GraphProcessingType}, pdg_spec::{ExportablePDG, ExportablePDGNode, PDGSpec}, sim_data_injection::TywavesInterface};
use serde::Deserialize;
use tauri::State;
use anyhow::{anyhow, Result};

use crate::{app_state::{AppState, GraphNodeHierarchy, HierarchicalGraph, ViewableGraph}, errors::map_err_to_string_async};

#[tauri::command]
pub async fn make_dpdg(state: State<'_, RwLock<AppState>>) -> Result<(), String> {
    map_err_to_string_async(async {
        let mut enable_grouping = false;
        {
            let pdg_config = {
                // Prevent global state lock during graph building.
                let state_guard = state.read().map_err(|_| anyhow::anyhow!("RwLock poisoned"))?;
                state_guard.pdg_config.clone()
            };

            let Some(pdg_config) = pdg_config else {
                anyhow::bail!("Tried building PDG before config was known.");
            };

            enable_grouping = pdg_config.group_nodes;
            
            // for _ in 0..100 {
            let start_time = SystemTime::now();
            let mut now = SystemTime::now();
            let reader = BufReader::new(File::open(&pdg_config.pdg_path)?);

            let mut deser = serde_json::Deserializer::from_reader(reader);
            deser.disable_recursion_limit();
            //serde_json::from_str::<PDGSpec>(buf.as_str())?;
            let pdg_raw = PDGSpec::deserialize(&mut deser)?;
            println!("Processing PDG with {} nodes and {} edges", pdg_raw.vertices.len(), pdg_raw.edges.len());
            let sliced = pdg_raw;

            println!("PDG read: {}", (now.elapsed().unwrap().as_nanos() as f64) / 1e6);
            now = SystemTime::now();

            println!("Read PDG from file");

            // First do a static slice to try to reduce the amount of analyzed nodes
            // let sliced = pdg_slice(pdg_raw, &pdg_config.criterion)?;

            // Build the DPDG
            let mut builder = GraphBuilder::new(&pdg_config.vcd_path, pdg_config.extra_scopes.clone(), sliced)?;
            let processing_type = if pdg_config.data_only { GraphProcessingType::DataOnly } else {GraphProcessingType::Normal };
            let dpdg = builder.process(&pdg_config.criterion, pdg_config.max_timesteps.map(|t| t as i64), processing_type)?;

            println!("DPDG build: {}", (now.elapsed().unwrap().as_nanos() as f64) / 1e6);
            now = SystemTime::now();
            println!("DPDG build complete");

            let dpdg = dpdg_make_exportable(dpdg);

            println!("Exportable: {}", (now.elapsed().unwrap().as_nanos() as f64) / 1e6);
            now = SystemTime::now();
            println!("Made DPDG exportable");

            // Convert to source language
            let mut converted_pdg = if !pdg_config.fir_repr {
                 pdg_convert_to_source(dpdg, false, true)
            } else {
                dpdg
            };

            println!("Conversion: {}", (now.elapsed().unwrap().as_nanos() as f64) / 1e6);
            now = SystemTime::now();
            println!("Converted to source representation");
            
            println!("DPDG has {} nodes and {} edges", converted_pdg.vertices.len(), converted_pdg.edges.len());

            // Add simulation data
            let tywaves = TywavesInterface::new(&pdg_config.hgldd_path, pdg_config.extra_scopes.clone(), &pdg_config.top_module)?;
        
            let tywaves_vcd_path = tywaves.vcd_rewrite(&pdg_config.vcd_path)?;
            println!("VCD rewrite done");
            tywaves.inject_sim_data(&mut converted_pdg, &tywaves_vcd_path)?;

            println!("Tywaves: {}", (now.elapsed().unwrap().as_nanos() as f64) / 1e6);

            for v in &mut converted_pdg.vertices {
                v.timestamp += 1;
            }

            println!("Total: {}", (start_time.elapsed().unwrap().as_nanos() as f64) / 1e6);

            //let converted_pdg = dpdg;

            println!("Data injection done");

            let (node_hierarchy, node_hierarchy_lookup) = if pdg_config.group_nodes {
                let (x, y) = build_node_hierarchy(&converted_pdg);
                (Some(x), Some(y))
            } else { (None, None) };

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
                dpdg: converted_pdg.clone(),
                shown_ids: (0..converted_pdg.vertices.len()).collect(),
                time_to_nodes,
                dep_to_edges,
                prov_to_edges,
                n_timestamps,
                source_files,
                should_group_nodes: pdg_config.group_nodes,
                node_hierarchy,
                node_hierarchy_lookup,
                current_hier_dpdg: None
            };

            let mut state_guard = state.write().map_err(|_| anyhow::anyhow!("RwLock poisoned"))?;
            state_guard.graph = Some(viewable_graph);
        } // Lock contention countermeasure
        // }
        if enable_grouping {
            rebuild_hier_graph(&state)?;
        }
        Ok(())
    }).await
}
 
/// Rebuilds the DPDG that is currently being displayed based on the hierarchical levels that are expanded.
pub fn rebuild_hier_graph(state: &State<'_, RwLock<AppState>>) -> Result<()> {
    let mut state_guard = state.write().map_err(|_| anyhow!("RwLock poisoned"))?;
    let Some(vgraph) = &mut state_guard.graph else {
        anyhow::bail!("Uninitialized graph!");
    };

    let Some(node_hier_lookup) = &vgraph.node_hierarchy_lookup else {
        anyhow::bail!("Uninitialized reverse hierarchy lookup!");
    };

    let pdg = &vgraph.dpdg;
    // First iterate over all the edges of the original DPDG. We will rebuild the entire node and edges list.
    // This is slow, but easy. On every edge, propagate upwards to find the highest collapsed hierarchical level.
    // Then check if we have already added this node to the new list. If so, take those indices, otherwise insert.
    // Then redirect the edge. At the end, deduplicate the edges.
    let mut node_to_index = HashMap::new();
    let mut new_nodes = vec![];
    let mut new_edges = HashSet::new();
    let mut original_ids = vec![];
    let mut group_ids = HashMap::new();

    for edge in &pdg.edges {
        // check if from node has a hierarchical node
        let from_hier = &node_hier_lookup[&(edge.from as usize)];
        let mut from_is_group = true;
        let from_pdg_node = get_highest_hier_node(&from_hier).unwrap_or_else(|| {
            from_is_group = false;
            vgraph.dpdg.vertices[edge.from as usize].clone() // otherwise, use the existing node
        });

        // same for 'to'
        let to_hier = &node_hier_lookup[&(edge.to as usize)];
        let mut to_is_group = true;
        let to_pdg_node = get_highest_hier_node(&to_hier).unwrap_or_else(|| {
            to_is_group = false;
            vgraph.dpdg.vertices[edge.to as usize].clone() // otherwise, use the existing node
        });

        // check if they already have an index, otherwise, insert
        let new_from_index = *node_to_index.entry(from_pdg_node.clone()).or_insert_with(|| {
            new_nodes.push(from_pdg_node);
            if from_is_group {
                group_ids.insert(new_nodes.len()-1, from_hier.clone());
                original_ids.push(from_hier.read().unwrap().group_id);
            } else {
                original_ids.push(edge.from as usize);
            }
            new_nodes.len()-1
        });
        

        let new_to_index = *node_to_index.entry(to_pdg_node.clone()).or_insert_with(|| {
            new_nodes.push(to_pdg_node);
            if to_is_group {
                group_ids.insert(new_nodes.len()-1, to_hier.clone());
                original_ids.push(to_hier.read().unwrap().group_id);
            } else {
                original_ids.push(edge.to as usize);
            }
            new_nodes.len()-1
        });

        if new_from_index == new_to_index {
            continue;
        }

        // insert redirected edge.
        let mut new_edge = edge.clone();
        new_edge.from = new_from_index as u32;
        new_edge.to = new_to_index as u32;

        new_edges.insert(new_edge);
    }

    let mut time_to_nodes = HashMap::new();
    for (idx, v) in new_nodes.iter().enumerate() {
        time_to_nodes.entry(v.timestamp).and_modify(|nodes: &mut Vec<usize>| nodes.push(idx)).or_insert(vec![idx]);
    }

    let mut dep_to_edges = HashMap::new();
    for (idx, e) in new_edges.iter().enumerate() {
        dep_to_edges.entry(e.from).and_modify(|edges: &mut Vec<usize>| edges.push(idx)).or_insert(vec![idx]);
    }

    let mut prov_to_edges = HashMap::new();
    for (idx, e) in new_edges.iter().enumerate() {
        prov_to_edges.entry(e.to).and_modify(|edges: &mut Vec<usize>| edges.push(idx)).or_insert(vec![idx]);
    }
    
    vgraph.current_hier_dpdg = Some(HierarchicalGraph {
        dpdg: ExportablePDG { vertices: new_nodes, edges: new_edges.into_iter().collect::<Vec<_>>() },
        group_ids,
        original_ids,
        time_to_nodes,
        dep_to_edges,
        prov_to_edges
    });

    Ok(())
}

fn get_highest_hier_node(hierarchy: &Arc<RwLock<GraphNodeHierarchy>>) -> Option<ExportablePDGNode> {
    let mut parent = hierarchy.clone();
    let mut highest_level = None;
    loop {
        let new_parent = {
            let guard = parent.read().unwrap();
            if !guard.expanded {
                highest_level = Some(guard.pdg_node.clone());
            }
            if let Some(p) = &guard.parent {
                if let Some(p) = p.upgrade() {
                    p.clone()
                } else {
                    break;
                }
            } else {
                break;
            }
        };
        parent = new_parent;
    }

    highest_level
}

fn create_hier_pdg_node(name: String, timestamp: i64, module_path: Vec<String>) -> ExportablePDGNode {
    ExportablePDGNode { file: "".into(), line: 0, char: 0, name, kind: chiseltrace_rs::pdg_spec::PDGSpecNodeKind::Definition, clocked: false, module_path, related_signal: None, sim_data: None, timestamp, is_chisel_assignment: false }
}

/// Builds a node hierarchy by first creating the hierarchy, then adding the nodes and making a reverse mapping
fn build_node_hierarchy(dpdg: &ExportablePDG) -> (Vec<Arc<RwLock<GraphNodeHierarchy>>>, HashMap<usize, Arc<RwLock<GraphNodeHierarchy>>>) {
    // Input: list of nodes with various levels of hierachies.
    // Desired output: Tree with the entire design hierarchy and indices for all the nodes in them
    // The main idea here is that for grouped mode there will be a separate graph. Upon each expand / collapse, the ViewablePDG will be rebuilt
    // Each edge will still contain references to node IDS. However, during the conversion process, for each node (which has a reference to the parent hierarchical group),
    // the lowest level expanded hierarchical group will be taken instead. If a group above the node in the hierarchy is collapsed,
    // the edge will instead be redirected to the group. After this phase, edges are deduplicated and visible nodes are calculated from the remaining edges

    // This is an arc because otherwise the compile complains (has to be send+sync for tokio I assume)
    let num_timestamps = dpdg.vertices.iter().map(|v| v.timestamp).max().unwrap();

    // We preemptively give the groups IDs. These IDs are vis.js IDs that will be used to draw the graph.
    let mut groups = vec![];
    let mut reverse_hier_lookup = HashMap::new();
    let global_id_offset = dpdg.vertices.len() * 5;
    let mut group_count = 0;
    for timestamp in 0..=num_timestamps {
        let top = Arc::new(RwLock::new(GraphNodeHierarchy { instance_name: "top".into(),
            expanded: true,
            pdg_node: create_hier_pdg_node("module_top".into(), timestamp, vec![]),
            node_indices: vec![],
            children: vec![],
            parent: None,
            group_id: global_id_offset + group_count
        }));
        group_count += 1;

        let mut unique_paths = HashSet::new();

        let filtered_nodes = dpdg.vertices
            .iter()
            .enumerate()
            .filter(|(_, v)| v.timestamp == timestamp);

        // Make a list of unique paths
        for (_, node) in filtered_nodes.clone() {
            unique_paths.insert(node.module_path.clone());
        }

        // Sort the paths by length, then create sub hierarchies
        let mut unique_paths = unique_paths.into_iter().collect::<Vec<_>>();
        unique_paths.sort_by_key(|p| p.len());

        for path in &unique_paths {
            let mut parent = top.clone();
            let (head, tail) = if path.len() > 1 {
                (&path[0..path.len()-1], &path[path.len()-1..])
            } else {
                (&[] as &[String], &path[..])
            };

            if tail.len() == 0 || tail[0] == "" {
                continue;
            }
            
            // Traverse the tree to the desired leaf where we want to add the new one
            for path_part in head {
                let new_parent = {
                    let mut parent_lock = parent.write().unwrap();
                    if let Some(p) = parent_lock.children.iter().find(|n| n.read().unwrap().instance_name.eq(path_part)) {
                        p.clone()
                    } else {
                        let mut my_modpath = parent_lock.pdg_node.module_path.clone();
                        my_modpath.push(path_part.clone());

                        parent_lock.children.push(Arc::new(RwLock::new(GraphNodeHierarchy {
                            instance_name: path_part.clone(),
                            expanded: false,
                            pdg_node: create_hier_pdg_node(format!("module_{}", path_part.clone()), timestamp, my_modpath),
                            node_indices: vec![],
                            children: vec![],
                            parent: Some(Arc::downgrade(&parent)),
                            group_id: global_id_offset + group_count
                        })));
                        group_count += 1;
                        parent_lock.children.last().unwrap().clone()
                    }
                };
                parent = new_parent;
            }

            // Now add the tail node
            let mut parent_lock = parent.write().unwrap();
            let mut my_modpath = parent_lock.pdg_node.module_path.clone();
            my_modpath.push(tail[0].clone());
            parent_lock.children.push(Arc::new(RwLock::new(GraphNodeHierarchy {
                instance_name: tail[0].clone(),
                expanded: false,
                pdg_node: create_hier_pdg_node(format!("module_{}", tail[0].clone()), timestamp, my_modpath),
                node_indices: vec![],
                children: vec![],
                parent: Some(Arc::downgrade(&parent)),
                group_id: global_id_offset + group_count
            })));
            group_count += 1;
        }

        // println!("{:#?}", top);

        // Add all the nodes to their correct group
        for (idx, node) in filtered_nodes {
            let mut parent = top.clone();
            for path_part in &node.module_path {
                if path_part == "" {
                    continue;
                }
                let new_parent = {
                    let parent_lock = parent.read().unwrap();
                    parent_lock.children.iter().find(|n| n.read().unwrap().instance_name.eq(path_part)).unwrap().clone()
                };
                parent = new_parent;
            }
            parent.write().unwrap().node_indices.push(idx);
            reverse_hier_lookup.insert(idx, parent.clone());
        }

        groups.push(top);
    }

    (groups, reverse_hier_lookup)
}