use std::{cell::RefCell, collections::HashMap, fs::{read_to_string, File}, io::BufWriter, rc::Rc};
use anyhow::{anyhow, Result};
use clap::Parser;
use errors::Error;
use pdg_spec::{LinkedPDGNode, PDGSpec, PDGSpecEdge};

mod pdg_spec;
mod errors;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The path to the input PDG
    path: String,

    /// The statement that should be used for the program slicing.
    slice_statement: String
}

fn main() -> Result<()> {
    let args = Args::parse();
    let buf = read_to_string(args.path)?;
    let pdg_raw = serde_json::from_str::<PDGSpec>(buf.as_str())?;

    // We now have the PDG in the form of two lists: vertices and edges
    // Now, we should turn it into a more suitable representation to work with it.

    // We first create a map for each 'from' node that lists all 'to' nodes
    let mut edge_map: HashMap<u32, Vec<u32>> = HashMap::new();
    for edge in &pdg_raw.edges {
        let destinations = pdg_raw.edges.iter()
            .filter(|e| e.from == edge.from)
            .map(|e| e.to)
            .collect::<Vec<_>>();
    
        edge_map.insert(edge.from, destinations);
    }

    let linkable_nodes = pdg_raw.vertices.iter().map(|e| Rc::new(RefCell::new(LinkedPDGNode::from(e)))).collect::<Vec<_>>();
    for (k, v) in edge_map {
        linkable_nodes[k as usize].borrow_mut().connections = v.iter().map(|i| linkable_nodes[*i as usize].clone()).collect::<Vec<_>>();
    }

    let stmt_idx = find_valid_statement(&linkable_nodes, &args.slice_statement)?;

    // Now traverse the dependency graph, starting from the slicing criterion
    let mut traversal_stack = vec![linkable_nodes[stmt_idx].clone()];
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
    let mut removed_indices = linkable_nodes.iter().enumerate()
        .filter(|(_,n)| !n.borrow().visited)
        .map(|(i,_)| i).collect::<Vec<_>>();
    
    println!("Started with {} nodes; Sliced node count: {}", pdg_raw.vertices.len(), pdg_raw.vertices.len() - removed_indices.len());

    // We now need to output the sliced PDG to json again. The easiest way to do this is to remove vertices and edges from the original list
    // and remapping the to and from in the edges.
    let mut idx_counter = 0;
    let mut idx_remap = Vec::new();
    for i in 0..pdg_raw.vertices.len() {
        if !removed_indices.contains(&i) {
            idx_remap.push(Some(idx_counter));
            idx_counter += 1;
        } else {
            idx_remap.push(None);
        }
    }

    let mut new_vertices = pdg_raw.vertices.clone();

    removed_indices.sort();
    removed_indices.reverse();

    // Might trigger a bunch of memcpy's but probably fine
    for i in removed_indices {
        new_vertices.remove(i);
    }

    let new_edges = pdg_raw.edges.iter().filter_map(|e| {
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

    let output_graph = PDGSpec{ vertices: new_vertices, edges: new_edges };

    let output_file = File::create("out_pdg.json")?;
    let writer = BufWriter::new(output_file);

    serde_json::to_writer_pretty(writer, &output_graph)?;

    Ok(())
}

fn find_valid_statement(nodes: &Vec<Rc<RefCell<LinkedPDGNode>>>, stmt: &String) -> Result<usize> {
    let idx = nodes.iter().position(|n| n.borrow().name.eq(stmt))
        .ok_or(anyhow!(Error::StatementLookupError(stmt.clone())))?;

    Ok(idx)
}
