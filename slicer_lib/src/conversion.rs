use std::{cell::RefCell, collections::{BTreeMap, HashMap, HashSet}, rc::Rc};
use itertools::Itertools;
use crate::{graphbuilder::DynPDGNode, pdg_spec::{ExportablePDG, ExportablePDGEdge, ExportablePDGNode, PDGSpecEdgeKind, PDGSpecNodeKind}};

pub fn pdg_convert_to_source(pdg: ExportablePDG) -> ExportablePDG {
    // Here, we convert the PDG from FIRRTL representation to source representation.
    // The only source information that is available is the source file and line mapping (TODO: ALSO CHECK THE CHARACTER INDEX!!)
    // Based on this, we can group nodes that belong to the same source statement. One issue is that
    // multiple source statements may exist on the same line. This is not yet addressed by this tool.
    // For example, also signals of type Bundle have the same source mapping to the definition of the entire bundle.
    // This will cause them to get grouped, which may not be desired.

    // First step is to make groups of vertices.
    let mut grouped_nodes: HashMap<(String, u32, u64), Vec<(ExportablePDGNode, usize)>> = HashMap::new();
    for (i, node) in pdg.vertices.iter().enumerate() {
        // This correction is needed to counteract the correction in the graphbuilder.
        // Basically, registers update, then the wires update. That means that a register update at t=x 
        // cannot have wire dependencies at t=x, they must be earlier. Therefore, a correction was introduced.
        // However, due to this correction, grouping was no longer working properly, so here we are.
        let group_timestamp = if node.clocked { node.timestamp } else { node.timestamp + 1 };
        grouped_nodes.entry((node.file.clone(), node.line, group_timestamp)).or_default().push((node.clone(), i));
    }

    // Redirect all Index edges. The probes will be removed in the next step
    let new_edges = pdg.edges.iter().flat_map(|e| {
        if e.kind == PDGSpecEdgeKind::Index {
            // Replace this edge.
            let mut stack = vec![e];
            let mut replacement_edges= vec![];
            while let Some(traversed_edge) = stack.pop() {
                replacement_edges.extend(pdg.edges.iter().filter(|r_e| r_e.from == traversed_edge.to && r_e.kind == PDGSpecEdgeKind::Data).cloned());
                stack.extend(pdg.edges.iter().filter(|r_e| r_e.from == traversed_edge.to && r_e.kind == PDGSpecEdgeKind::Index));
            }
            for r_e in &mut replacement_edges {
                r_e.from = e.from;
                r_e.clocked = e.clocked;
                r_e.kind = PDGSpecEdgeKind::Index;
            }
            // println!("{:#?}", replacement_edges);
            replacement_edges
        } else if !pdg.vertices[e.from as usize].name.starts_with("defnode_probe") { // Should definitely be replaced in the future
            vec![e.clone()]
        } else { vec![] }
    }).collect::<Vec<_>>();

    // println!("{:#?}", new_edges);

    // For every group, check vertex reachability within the group and split if necessary.
    // This is required for split compound signals. This code is probably real slow, optimise as needed
    let groups = grouped_nodes.values().flat_map(|g| {
        let mut grouped_nodes = BTreeMap::from_iter(g.iter().map(|n| (n.1, n.0.clone())));
        let mut groups = vec![];
        while let Some(current_node) = grouped_nodes.pop_first() {
            let mut stack = vec![(current_node.1, current_node.0)];
            let mut current_group = vec![];
            while let Some((visited_node, visited_idx)) = stack.pop() {
                let to_add = new_edges.iter().filter(|e| 
                    (e.to == visited_idx as u32 || e.from == visited_idx as u32) && e.kind != PDGSpecEdgeKind::Index);
                for edge in to_add {
                    if let Some(x) = grouped_nodes.remove(&(edge.from as usize)) {
                        stack.push((x, edge.from as usize));
                    }
                    if let Some(x) = grouped_nodes.remove(&(edge.to as usize)) {
                        stack.push((x, edge.to as usize));
                    }
                }
                if !visited_node.name.starts_with("defnode_probe") {// Bad solution, should probably replace it with something better.
                    current_group.push((visited_node, visited_idx));
                }
            }
            if !current_group.is_empty() {
                groups.push(current_group);
            }
        }
        groups
    }).collect::<Vec<_>>(); 

    // Map the old vertex indices to the newly grouped ones.
    let edgemap = groups.iter().enumerate().flat_map(|(new_i, g)| {
        let own_indices = g.iter().map(|v| v.1); // Indices are guaranteed to be unique
        own_indices.map(move |idx| (idx as u32, new_i as u32))
    }).collect::<HashMap<_,_>>();

    // Filters out any intra-group edges. There is one problem with this: there may actually be a clocked self dependency
    // somewhere in the grouped nodes. We should check from each group and add it if needed.
    // Also, we need to dedup the edges.
    let outgoing_edges = new_edges.iter()
        .map(|e| ExportablePDGEdge{from: edgemap[&e.from], to: edgemap[&e.to], ..e.clone()})
        .filter(|e| {
        // If both to and from point to the same group, remove the edge.
        e.to != e.from
    }).unique();

    let self_dependencies = groups.iter().filter_map(|g| {
        let own_index = g[0].1 as u32;
        // Simple DFS to find if there is a clocked cycle in the group
        let group_indices = g.iter().map(|n| n.1 as u32).unique().collect::<Vec<_>>();
        let group_edges = new_edges
            .iter()
            .filter(|e| group_indices.contains(&e.from) && group_indices.contains(&e.to))
            .unique_by(|e| (e.from, e.to))
            .collect::<Vec<_>>();
        
        let mut cycle_found = false;
        let mut visited = std::collections::HashSet::new();
        
        'outer: for &node_idx in &group_indices {
            if visited.contains(&node_idx) {
                continue;
            }
            let mut dfs_stack = vec![(node_idx, false, vec![node_idx])]; // (current_node, clocked_found, visited)
        
            while let Some((current_el, clocked_found, path)) = dfs_stack.pop() {
                if visited.contains(&current_el) {
                    continue;
                }
                visited.insert(current_el);
        
                for edge in &group_edges {
                    if edge.from == current_el {
                        if path.contains(&edge.to) {
                            // Cycle found
                            if edge.clocked || clocked_found {
                                cycle_found = true;
                                break 'outer;
                            }
                        } else {
                            dfs_stack.push((edge.to, edge.clocked || clocked_found, path.iter().cloned().chain(std::iter::once(edge.to)).collect()));
                        }
                    }
                }
            }
        }

        if cycle_found {
            Some(ExportablePDGEdge {from: edgemap[&own_index], to: edgemap[&own_index], kind: PDGSpecEdgeKind::Data, clocked: true})
        } else {
            None
        }
    });

    let new_verts = groups.iter().map(|g| {
        let contains_data = g.iter().any(|(v,_)| v.kind == PDGSpecNodeKind::DataDefinition || v.kind == PDGSpecNodeKind::Connection);
        let contains_cond = g.iter().any(|(v,_)| v.kind == PDGSpecNodeKind::ControlFlow);
        let contains_io = g.iter().any(|(v,_)| v.kind == PDGSpecNodeKind::IO);

        let vert_kind = if contains_io {
            PDGSpecNodeKind::IO
        } else if contains_data {
            PDGSpecNodeKind::Connection
        } else if contains_cond {
            PDGSpecNodeKind::ControlFlow
        } else {
            PDGSpecNodeKind::Definition
        };
        let v0 = &g[0].0;
        let filename = v0.file.split("/").last().unwrap();
        let primary_statement = g.iter().find(|n| n.0.is_chisel_assignment);
        let node_name = if let Some((stmt, _)) = primary_statement {
            format!("{} ({}:{})", stmt.name, filename, stmt.line)
        } else {
            format!("{}:{}", filename , v0.line)
        };
        ExportablePDGNode {name: node_name, kind: vert_kind, ..v0.clone()}
    }).collect::<Vec<_>>();

    let merged_edges = outgoing_edges.chain(self_dependencies)
    .map(|e| {
        // Some edges may be unjustly marked as non-clocked. We need to restore them
        ExportablePDGEdge { clocked: new_verts[e.from as usize].clocked, ..e }
    })  
    .unique()
    .collect::<Vec<_>>();

    ExportablePDG {
        vertices: new_verts,
        edges: merged_edges
    }
}

pub fn dpdg_make_exportable(root: Rc<RefCell<DynPDGNode>>) -> ExportablePDG {
    let mut pdg = ExportablePDG::empty();
    // We keep track of the nodes we have seen so far. If we encounter a new node, we add it to the scanned nodes.
    // If we encounter a node that was previously scanned, we use that nodes index instead.
    let mut scanned_nodes = vec![];
    let mut edges = HashSet::new();

    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let this_idx = if let Some((idx, _)) = scanned_nodes.iter().find_position(|el| Rc::ptr_eq(el, &node)) {
            idx
        } else {
            scanned_nodes.push(node.clone());
            scanned_nodes.len()-1
        };

        let borrowed_node = node.borrow();
        for (dep, kind) in &borrowed_node.dependencies {
            let dep_idx = if let Some((idx, _)) = scanned_nodes.iter().find_position(|el| Rc::ptr_eq(el, dep)) {
                idx
            } else {
                scanned_nodes.push(dep.clone());
                scanned_nodes.len()-1
            };
            
            edges.insert(ExportablePDGEdge { from: this_idx as u32, to: dep_idx as u32, kind: *kind, clocked: borrowed_node.inner.clocked });
            stack.push(dep.clone());
        }
    }

    let pdg_verts = scanned_nodes.iter().map(|el| {
        let node = el.borrow();
        ExportablePDGNode { name: format!("{} at t={}", node.inner.name, node.timestamp), timestamp: node.timestamp, ..node.inner.clone().into()}
    }).collect::<Vec<_>>();

    pdg.vertices = pdg_verts;
    pdg.edges = edges.into_iter().collect::<Vec<_>>();

    pdg
}