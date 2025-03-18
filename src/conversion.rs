use std::{collections::HashMap, fs::File, io::BufWriter};
use anyhow::Result;
use itertools::Itertools;
use crate::pdg_spec::{PDGSpec, PDGSpecEdge, PDGSpecEdgeKind, PDGSpecNode, PDGSpecNodeKind};

pub fn pdg_convert(pdg: PDGSpec) -> Result<()> {
    // Here, we convert the PDG from FIRRTL representation to source representation.
    // The only source information that is available is the source file and line mapping (TODO: ALSO CHECK THE CHARACTER INDEX!!)
    // Based on this, we can group nodes that belong to the same source statement. One issue is that
    // multiple source statements may exist on the same line. This is not yet addressed by this tool.
    // For example, also signals of type Bundle have the same source mapping to the definition of the entire bundle.
    // This will cause them to get grouped, which may not be desired.

    // First step is to make groups of vertices.
    let mut grouped_nodes: HashMap<(String, u32), Vec<(PDGSpecNode, usize)>> = HashMap::new();
    for (i, node) in pdg.vertices.iter().enumerate() {
        grouped_nodes.entry((node.file.clone(), node.line)).or_default().push((node.clone(), i));
    }

    // Guarantee deterministic traversal
    let groups = grouped_nodes.values().collect::<Vec<_>>();

    // Map the old vertex indices to the newly grouped ones.
    let edgemap = groups.iter().enumerate().flat_map(|(new_i, g)| {
        let own_indices = g.iter().map(|v| v.1); // Indices are guaranteed to be unique
        own_indices.map(move |idx| (idx as u32, new_i as u32))
    }).collect::<HashMap<_,_>>();

    // Filters out any intra-group edges. There is one problem with this: there may actually be a clocked self dependency
    // somewhere in the grouped nodes. We should check from each group and add it if needed.
    // Also, we need to dedup the edges.
    let outgoing_edges = pdg.edges.iter()
        .map(|e| PDGSpecEdge{from: edgemap[&e.from], to: edgemap[&e.to], ..e.clone()})
        .filter(|e| {
        // If both to and from point to the same group, remove the edge.
        e.to != e.from
    }).unique();

    let self_dependencies = groups.iter().filter_map(|g| {
        let own_index = g[0].1 as u32;
        if pdg.edges.iter().any(|e| (edgemap[&e.to] == own_index) && (edgemap[&e.from] == own_index) && e.clocked && e.kind == PDGSpecEdgeKind::Data) {
            Some(PDGSpecEdge {from: own_index, to: own_index, kind: PDGSpecEdgeKind::Data, clocked: true})
        } else {
            None
        }
    });

    let merged_edges = outgoing_edges.chain(self_dependencies).collect::<Vec<_>>();

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
        PDGSpecNode {name: format!("{}:{}", v0.file.split("/").last().unwrap(), v0.line), kind: vert_kind, ..v0.clone()}
    }).collect::<Vec<_>>();

    let converted_pdg = PDGSpec {
        vertices: new_verts,
        edges: merged_edges,
        predicates: pdg.predicates,
        cfg: pdg.cfg
    };

    let output_file = File::create("out_chisel_pdg.json")?;
    let writer = BufWriter::new(output_file);

    serde_json::to_writer_pretty(writer, &converted_pdg)?;
    Ok(())
}