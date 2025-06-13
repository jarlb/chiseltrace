use std::{cell::RefCell, collections::{BTreeMap, HashMap, HashSet}, rc::Rc};
use itertools::Itertools;
use crate::{graphbuilder::DynPDGNode, pdg_spec::{ExportablePDG, ExportablePDGEdge, ExportablePDGNode, PDGSpecEdgeKind, PDGSpecNodeKind}};

pub fn pdg_convert_to_source(pdg: ExportablePDG, verbose_name: bool, is_dpdg: bool) -> ExportablePDG {
    // Here, we convert the PDG from FIRRTL representation to source representation.
    // The only source information that is available is the source file and line mapping (TODO: ALSO CHECK THE CHARACTER INDEX!!)
    // Based on this, we can group nodes that belong to the same source statement. One issue is that
    // multiple source statements may exist on the same line. This is not yet addressed by this tool.
    // For example, also signals of type Bundle have the same source mapping to the definition of the entire bundle.
    // This will cause them to get grouped, which may not be desired.

    // First step is to make groups of vertices.
    let mut grouped_nodes: HashMap<(String, u32, i64), Vec<(ExportablePDGNode, usize)>> = HashMap::new();
    for (i, node) in pdg.vertices.iter().enumerate() {
        // This correction is needed to counteract the correction in the graphbuilder.
        // Basically, registers update, then the wires update. That means that a register update at t=x 
        // cannot have wire dependencies at t=x, they must be earlier. Therefore, a correction was introduced.
        // However, due to this correction, grouping was no longer working properly, so here we are.
        let group_timestamp = if node.clocked { node.timestamp } else { node.timestamp + 1 };
        grouped_nodes.entry((node.file.clone(), node.line, group_timestamp)).or_default().push((node.clone(), i));
    }


    // TODO: the bug comes from the fact that the timestamp is saturated to 0, so the grouper tries to group the initial value wire
    // with other t=1 nodes, but there is no vertex reachability on the same timestamp, so it doesn't group properly.
    // The solution would be to switch to i64 for timestamps (bad idea), or deploy a hotfix that would scan for non-chisel nodes at group time 1,
    // then put them on group time 0 (also not great)

    // Performance optimization: Pre-compute mapping between edge.from and a list of edges.
    // It used to be pdg.edges.iter().filter(|r_e| r_e.from == traversed_edge.to && r_e.kind == PDGSpecEdgeKind::Data).cloned()).
    // This is wildly inefficient and causes O(N^2) complexity. By pre-computing the filter using a hashmap, we reduce to O(N)

    let mut edges_by_from: HashMap<u32, Vec<_>> = HashMap::new();
    for edge in &pdg.edges {
        edges_by_from.entry(edge.from).and_modify(|x| x.push(edge)).or_insert(vec![edge]);
    }

    // Redirect all Index edges. The probes will be removed in the next step
    let new_edges = pdg.edges.iter().flat_map(|e| {
        if e.kind == PDGSpecEdgeKind::Index {
            // Replace this edge.
            let mut stack = vec![e];
            let mut replacement_edges= vec![];
            while let Some(traversed_edge) = stack.pop() {
                // We use a stack graph traversal because a probe node may itself have an index dependency. If that is the case,
                // we need to squash them
                let Some(r_e) = edges_by_from.get(&traversed_edge.to) else {
                    continue;
                };

                replacement_edges.extend(r_e.iter().filter(|r_e| r_e.kind == PDGSpecEdgeKind::Data).map(|x| *x).cloned());
                stack.extend(r_e.iter().filter(|r_e| r_e.kind == PDGSpecEdgeKind::Index));
            }
            for r_e in &mut replacement_edges {
                r_e.from = e.from;
                r_e.clocked = e.clocked;
                r_e.kind = PDGSpecEdgeKind::Index;
            }
            // println!("{:#?}", replacement_edges);
            replacement_edges
        } else if !pdg.vertices[e.from as usize].name.starts_with("defnode_probe") { // Should definitely be replaced in the future
            // Filter away edges that come from a probe node
            vec![e.clone()]
        } else { vec![] }
    }).collect::<Vec<_>>();


    // Same thing here: filtering explodes time complexity, replace with HashMap
    let mut edges_by_to: HashMap<u32, Vec<_>> = HashMap::new();
    edges_by_from.clear();
    for edge in &new_edges {
        edges_by_from.entry(edge.from).and_modify(|x| x.push(edge)).or_insert(vec![edge]);
        edges_by_to.entry(edge.to).and_modify(|x| x.push(edge)).or_insert(vec![edge]);
    }


    // For every group, check vertex reachability within the group and split if necessary.
    // This is required for split compound signals.
    let groups = grouped_nodes.values().flat_map(|g| {
        let mut grouped_nodes = BTreeMap::from_iter(g.iter().map(|n| (n.1, n.0.clone())));
        let mut groups = vec![];
        while let Some(current_node) = grouped_nodes.pop_first() {
            let mut stack = vec![(current_node.1, current_node.0)];
            let mut current_group = vec![];
            while let Some((visited_node, visited_idx)) = stack.pop() {

                // let to_add = new_edges.iter().filter(|e| 
                //     (e.to == visited_idx as u32 || e.from == visited_idx as u32) && e.kind != PDGSpecEdgeKind::Index);
                // let to_add = edges_by_from.get(&(visited_idx as u32)).into_iter().flatten().chain(
                //     edges_by_to.get(&(visited_idx as u32)).into_iter().flatten()
                // ).filter(|e| e.kind != PDGSpecEdgeKind::Index);

                let to_add = edges_by_from
                    .get(&(visited_idx as u32))
                    .into_iter()
                    .flatten()
                    .chain(
                        edges_by_to.get(&(visited_idx as u32))
                        .into_iter()
                        .flatten()
                        .filter(|x| x.from != x.to) // Do not process self-referring edges twice
                    )
                    .filter(|e| e.kind != PDGSpecEdgeKind::Index);

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
    });

    // This can only occur when the PDG being converted is a static PDG. The DPDG is a DAG, so this does not happen
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

        let primary_statement = g.iter().find(|n| n.0.is_chisel_assignment);


        let vert_kind = if let Some(stmt) = primary_statement {
            stmt.0.kind
        } else {
            if contains_io {
                PDGSpecNodeKind::IO
            } else if contains_data {
                PDGSpecNodeKind::Connection
            } else if contains_cond {
                PDGSpecNodeKind::ControlFlow
            } else {
                PDGSpecNodeKind::Definition
            }  
        };
        let v0 = &g[0].0;
        let filename = v0.file.split("/").last().unwrap();
        let node_name = if let Some((stmt, _)) = primary_statement {
            if verbose_name {
                format!("{} at t={} ({}:{})", stmt.name, stmt.timestamp, filename, stmt.line)
            } else {
                stmt.name.clone()
            }
        } else {
            format!("{}:{}", filename , v0.line)
        };
        ExportablePDGNode {name: node_name, kind: vert_kind, ..v0.clone()}
    }).collect::<Vec<_>>();

    let merged_edges = if is_dpdg {
        outgoing_edges.map(|e| {
            // Some edges may be unjustly marked as non-clocked. We need to restore them
            ExportablePDGEdge { clocked: new_verts[e.from as usize].clocked, ..e }
        })  
        .unique().collect::<Vec<_>>()
    } else {
        outgoing_edges.chain(self_dependencies).map(|e| {
            // Some edges may be unjustly marked as non-clocked. We need to restore them
            ExportablePDGEdge { clocked: new_verts[e.from as usize].clocked, ..e }
        })  
        .unique().collect::<Vec<_>>()
    };

    // There is one final problem: some constructs, such as lookup tables may generate an enormous amount of nodes.
    // Most of these have been merged at this point, but there may still be some that are not. These nodes are
    // marked as non-chisel statements and can therefore not contain simulation data. It is best to merge them for clarity.
    // To do this, we will search each timestep for duplicate non-chisel statements. We will then merge them into one and
    // dedup any edges.

    // A note: this processing step can result in erroneous behaviour in designs where an anonymous statement is in multiple
    // data paths, such as an adder tree.
    // To specifically target lookup tables, the following heuristic is used: the merged nodes must have the same dependencies.

    edges_by_to.clear();
    edges_by_from.clear();
    for edge in &merged_edges {
        edges_by_from.entry(edge.from).and_modify(|x| x.push(edge)).or_insert(vec![edge]);
        edges_by_to.entry(edge.to).and_modify(|x| x.push(edge)).or_insert(vec![edge]);
    }

    let mut removed_indices = vec![];
    let mut processed_verts: HashMap<i64, Vec<(&ExportablePDGNode, usize)>> = HashMap::new();
    for (vert_idx, vert) in new_verts.iter().enumerate() {
        processed_verts.entry(vert.timestamp)
            .and_modify(|vs| {
                if let Some((_, dup_idx)) = vs.iter().find(|(v, _)| !v.is_chisel_assignment &&
                v.file == vert.file && v.line == vert.line && v.name == vert.name) {
                    // There is a duplicate vertex. Now we have to check if the edges are the same.
                    // If so, we discard.
                    let mut connected_edges_dup = HashSet::new();
                    for check_edge in edges_by_from.get(&(*dup_idx as u32)).into_iter().flatten() {
                        connected_edges_dup.insert(check_edge.to);
                    }

                    let mut duplicate = true;
                    for check_edge in edges_by_from.get(&(vert_idx as u32)).into_iter().flatten() {
                        if !connected_edges_dup.contains(&check_edge.to) {
                            duplicate = false;
                            break;
                        }
                    }

                    if duplicate {
                        removed_indices.push(vert_idx);
                    }
                } else {
                    // If for the verts timestamp, we do not find a vert that is same, we add the vert to the processed verts.
                    // If needed, we could check for the edges too (they will have the same ones), but I don't think it is necessary.
                    vs.push((vert, vert_idx));
                }
            })
            .or_insert(vec![(vert, vert_idx)]);
    }

    // Now we just delete the duplicate verts.
    removed_indices.sort();
    removed_indices.reverse();
    let mut pruned_verts = new_verts.clone();
    for i in &removed_indices {
        pruned_verts.remove(*i);
    }

    // and update the edges as well
    let mut edge_remap: HashMap<usize, Option<usize>> = HashMap::new();
    let mut removed_counter = 0;
    for i in 0..new_verts.len() {
        edge_remap.insert(i,if removed_indices.contains(&i) {
            removed_counter += 1;
            None
        } else {
            Some(i - removed_counter)
        });
    }

    let remapped_edges = merged_edges.iter().filter_map(|e| {
        let remap_to = edge_remap[&(e.to as usize)];
        let remap_from = edge_remap[&(e.from as usize)];

        if let (Some(to), Some(from)) = (remap_to, remap_from) {
            Some(ExportablePDGEdge{from: from as u32, to: to as u32, ..e.clone()})
        } else {
            None
        }
    }).collect::<Vec<_>>();

    ExportablePDG {
        vertices: pruned_verts,
        edges: remapped_edges
    }
}

/// A data structure that aids in converting linked graphs into 2 list representation
struct LinkedNodeSet<T> {
    nodes: Vec<Rc<T>>,
    index_map: HashMap<*const T, usize>
}

impl<T> LinkedNodeSet<T> {
    fn new() -> Self {
        LinkedNodeSet { nodes: vec![], index_map: HashMap::new() }
    }

    fn find_position(&mut self, node: &Rc<T>) -> Option<usize> {
        // We do a lookup in a hashmap based on the pointer. Without this, we would have to do linear search.
        // That would be O(N^2) and explodes on larger graphs. We cannot just use a Set, because the ordering is important
        self.index_map.get(&Rc::as_ptr(node)).copied()
    }

    fn push(&mut self, node: &Rc<T>) -> usize {
        let ptr = Rc::as_ptr(node);
        *self.index_map.entry(ptr).or_insert_with(|| {
            let idx = self.nodes.len();
            self.nodes.push(node.clone());
            idx
        })
    }
}

pub fn dpdg_make_exportable(root: Rc<RefCell<DynPDGNode>>) -> ExportablePDG {
    let mut pdg = ExportablePDG::empty();
    // We keep track of the nodes we have seen so far. If we encounter a new node, we add it to the scanned nodes.
    // If we encounter a node that was previously scanned, we use that nodes index instead.
    let mut scanned_nodes = LinkedNodeSet::new();
    let mut edges = HashSet::new();

    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        // let this_idx = if let Some((idx, _)) = scanned_nodes.iter().find_position(|el| Rc::ptr_eq(el, &node)) {
        //     idx
        // } else {
        //     scanned_nodes.push(node.clone());
        //     scanned_nodes.len()-1
        // };

        let this_idx = if let Some(idx) = scanned_nodes.find_position(&node) {
            idx
        } else {
            scanned_nodes.push(&node)
        };

        let borrowed_node = node.borrow();
        for (dep, kind) in &borrowed_node.dependencies {
            let dep_idx = if let Some(idx) = scanned_nodes.find_position(&dep) {
                idx
            } else {
                stack.push(dep.clone());
                scanned_nodes.push(&dep)
            };
            
            edges.insert(ExportablePDGEdge { from: this_idx as u32, to: dep_idx as u32, kind: *kind, clocked: borrowed_node.inner.clocked });
        }
    }

    let pdg_verts = scanned_nodes.nodes.iter().map(|el| {
        let node = el.borrow();
        ExportablePDGNode { name: format!("{}", node.inner.name), timestamp: node.timestamp, ..(*node.inner).clone().into()}
    }).collect::<Vec<_>>();

    pdg.vertices = pdg_verts;
    pdg.edges = edges.into_iter().collect::<Vec<_>>();

    pdg
}