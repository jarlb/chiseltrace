use crate::pdg_spec::{PDGSpec, PDGSpecNode};

pub struct CFGStatement {
    stmt: PDGSpecNode
}

pub struct CFGFork {
    predicate: PDGSpecNode,
    true_branch: Vec<CFGNode>,
    false_branch: Vec<CFGNode>
}

pub enum CFGNode {
    Statement(CFGStatement),
    Fork(CFGFork)
}

pub struct CFG {
    nodes: Vec<CFGNode>
}

// impl CFG {
//     pub fn from_pdg(pdg: &PDGSpec) -> Self {
//         pdg.cfg
//     }
// }
