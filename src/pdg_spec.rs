use std::{cell::RefCell, rc::Rc};

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct PDGSpec {
    pub vertices: Vec<PDGSpecNode>,
    pub edges: Vec<PDGSpecEdge>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PDGSpecNode {
    pub file: String,
    pub line: u32,
    pub name: String,
    pub kind: PDGSpecNodeKind
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum PDGSpecNodeKind {
    Definition,
    DataDefinition,
    IO,
    Connection,
    ControlFlow,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PDGSpecEdge {
    pub from: u32,
    pub to: u32,
    pub kind: PDGSpecEdgeKind,
    pub clocked: bool
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum PDGSpecEdgeKind {
    Data,
    Conditional,
    Declaration
}

// Warning: do not debug print this using the standard trait implementation, it is a linked structure and it will result in infinite recursion
pub struct LinkedPDGNode {
    pub file: String,
    pub line: u32,
    pub name: String,
    pub kind: PDGSpecNodeKind,
    pub connections: Vec<Rc<RefCell<LinkedPDGNode>>>,
    pub visited: bool
}

impl From<&PDGSpecNode> for LinkedPDGNode {
    fn from(value: &PDGSpecNode) -> Self {
        LinkedPDGNode { file: value.file.clone(), line: value.line, name: value.name.clone(), kind: value.kind, connections: Vec::new(), visited: false }
    }
}