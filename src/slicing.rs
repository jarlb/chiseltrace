use std::path::Path;
use std::{cell::RefCell, collections::HashMap, fs::File, io::BufWriter, rc::Rc};
use anyhow::{anyhow, Result};
use crate::cfg::CFGStatement;
use crate::errors::Error;
use crate::pdg_spec::{CFGSpecStatement, LinkedPDGNode, PDGSpec, PDGSpecEdge};

/// Function that takes in a PDG in Spec form (i.e. separate vertices and edge lists, linked by indices)
/// and produces a list of vertices that refer to their dependence nodes.
pub fn link_pdg(pdg: &PDGSpec) -> Vec<Rc<RefCell<LinkedPDGNode>>> {
    // We first create a map for each 'from' node that lists all 'to' nodes
    let mut edge_map: HashMap<u32, Vec<u32>> = HashMap::new();
    for edge in &pdg.edges {
        let destinations = pdg.edges.iter()
            .filter(|e| e.from == edge.from)
            .map(|e| e.to)
            .collect::<Vec<_>>();
    
        edge_map.insert(edge.from, destinations);
    }

    let linkable_nodes = pdg.vertices.iter().map(|e| Rc::new(RefCell::new(LinkedPDGNode::from(e)))).collect::<Vec<_>>();
    for (k, v) in edge_map {
        linkable_nodes[k as usize].borrow_mut().connections = v.iter().map(|i| linkable_nodes[*i as usize].clone()).collect::<Vec<_>>();
    }

    linkable_nodes
}

/// Function that finds the indices of the removed nodes (1) and provides a mapping from old indices
/// to new ones
pub fn get_edge_replacement_mapping(linkable_nodes: &Vec<Rc<RefCell<LinkedPDGNode>>>, criterion_idx: usize) -> (Vec<usize>, Vec<Option<u32>>) {
    // Now traverse the dependency graph, starting from the slicing criterion
    let mut traversal_stack = vec![linkable_nodes[criterion_idx].clone()];
    while let Some(node) = traversal_stack.pop() {
        node.borrow_mut().visited = true;

        for el in &node.borrow().connections {
            if !el.borrow().visited {
                traversal_stack.push(el.clone());
            }
        }
    }

    // It is important to realize that these indices are the same as the original vertices, therefore we can use the indices of
    // the linked nodes to slice the original.
    let removed_indices = linkable_nodes.iter().enumerate()
        .filter(|(_,n)| !n.borrow().visited)
        .map(|(i,_)| i).collect::<Vec<_>>();

    // We now need to output the sliced PDG to json again. The easiest way to do this is to remove vertices and edges from the original list
    // and remapping the to and from in the edges.
    let mut idx_counter = 0;
    let mut idx_remap = Vec::new();
    for i in 0..linkable_nodes.len() {
        if !removed_indices.contains(&i) {
            idx_remap.push(Some(idx_counter));
            idx_counter += 1;
        } else {
            idx_remap.push(None);
        }
    }

    (removed_indices, idx_remap)
}

/// Reduces a CFG by removing all statements that that have an index that is included in the provided list of indices to be removed
/// Furthermore, all predicate probe vertices assaciated with any removed predicates will be removed from the PDG
/// Returns a new PDGSpec
pub fn reduce_cfg(pdg: PDGSpec, removed_indices: &Vec<usize>, idx_remap: &Vec<Option<u32>>) -> PDGSpec {
    let mut removed_predicates_idx: Vec<usize> = vec![];
    let reduced_cfg = remove_cfg_statements(pdg.cfg, removed_indices, idx_remap, &mut removed_predicates_idx);
    // Since in the first removal pass, we can't remove predicate probe vertices,
    // we can use the amount of removed indices in the first pass as an offset for this one.
    removed_predicates_idx.sort();
    removed_predicates_idx.reverse();
    let mut new_preds = pdg.predicates.clone();
    for i in removed_predicates_idx {
        new_preds.remove(i);
    }
    // No need to remove edges, they shouldn't be affected
    PDGSpec { vertices: pdg.vertices, edges: pdg.edges, predicates: new_preds, cfg: reduced_cfg }
}

fn remove_cfg_statements(cfg: Vec<CFGSpecStatement>, remove_idx: &Vec<usize>, idx_remap: &Vec<Option<u32>>, removed_predicates: &mut Vec<usize>) -> Vec<CFGSpecStatement> {
    cfg.iter().filter_map(|s| {
        let should_keep = !remove_idx.contains(&(s.stmtRef as usize));
        if let Some(pred_stmt) = s.predStmtRef {
            // The statement is a conditional fork
            if should_keep {
                // Process the branches
                let new_true_branch = s.trueBranch.as_ref().map(|true_branch| remove_cfg_statements(true_branch.clone(), remove_idx, idx_remap, removed_predicates));
                let new_false_branch = s.falseBranch.as_ref().map(|false_branch| remove_cfg_statements(false_branch.clone(), remove_idx, idx_remap, removed_predicates));
                let new_stmt_ref = idx_remap[s.stmtRef as usize].unwrap();
                Some(CFGSpecStatement{stmtRef: new_stmt_ref, trueBranch: new_true_branch, falseBranch: new_false_branch, ..s.clone()})
            } else {
                removed_predicates.push(pred_stmt as usize);
                None
            }
        } else {
            should_keep.then(|| {
                let new_stmt_ref = idx_remap[s.stmtRef as usize]?;
                Some(CFGSpecStatement { stmtRef: new_stmt_ref, ..s.clone() })
            }).flatten()
        }
        
    }).collect::<Vec<_>>()
}

pub fn pdg_slice(pdg: PDGSpec, criterion: &str) -> Result<PDGSpec> {
    // We now have the PDG in the form of two lists: vertices and edges
    // Now, we should turn it into a more suitable representation to work with it.

    let linkable_nodes = link_pdg(&pdg);

    // Check if the criterion is even in the pdg
    let stmt_idx = find_valid_statement(&linkable_nodes, criterion)?;

    let (mut removed_indices, idx_remap) = get_edge_replacement_mapping(&linkable_nodes, stmt_idx);

    // TODO: remove
    println!("Started with {} nodes; Sliced node count: {}", pdg.vertices.len(), pdg.vertices.len() - removed_indices.len());

    let mut new_vertices = pdg.vertices.clone();

    removed_indices.sort();
    removed_indices.reverse();

    // Might trigger a bunch of memcpy's but probably fine
    for i in &removed_indices {
        new_vertices.remove(*i);
    }

    let new_edges = pdg.edges.iter().filter_map(|e| {
        if let (Some(from), Some(to)) = (idx_remap[e.from as usize], idx_remap[e.to as usize]) {
            Some(PDGSpecEdge{
                from,
                to,
                ..e.clone()
            })
        } else {
            None
        }
    }).collect::<Vec<_>>();

    let new_pdg = reduce_cfg(PDGSpec{ vertices: new_vertices, edges: new_edges, predicates: pdg.predicates, cfg: pdg.cfg }, &removed_indices, &idx_remap);

    Ok(new_pdg)
}

pub fn write_pdg<P: AsRef<Path>>(pdg: &PDGSpec, path: P) -> Result<()> {

    let output_file = File::create(path)?;
    let writer = BufWriter::new(output_file);

    serde_json::to_writer_pretty(writer, pdg)?;
    Ok(())
}

fn find_valid_statement(nodes: &Vec<Rc<RefCell<LinkedPDGNode>>>, stmt: &str) -> Result<usize> {
    let idx = nodes.iter().position(|n| n.borrow().name.eq(stmt))
        .ok_or(anyhow!(Error::StatementLookupError(stmt.to_string())))?;

    Ok(idx)
}
