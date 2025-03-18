use std::{cell::RefCell, rc::Rc};

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct PDGSpec {
    pub vertices: Vec<PDGSpecNode>,
    pub edges: Vec<PDGSpecEdge>,
    pub predicates: Vec<PDGSpecNode>,
    pub cfg: Vec<CFGSpecStatement>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PDGSpecNode {
    pub file: String,
    pub line: u32,
    pub char: u32,
    pub name: String,
    pub kind: PDGSpecNodeKind
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum PDGSpecNodeKind {
    Definition,
    DataDefinition,
    IO,
    Connection,
    ControlFlow,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PDGSpecEdge {
    pub from: u32,
    pub to: u32,
    pub kind: PDGSpecEdgeKind,
    pub clocked: bool
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PDGSpecEdgeKind {
    Data,
    Conditional,
    Declaration
}

// Warning: do not debug print this using the standard trait implementation, it is a linked structure and it will result in infinite recursion
pub struct LinkedPDGNode {
    pub _file: String,
    pub _line: u32,
    pub name: String,
    pub _kind: PDGSpecNodeKind,
    pub connections: Vec<Rc<RefCell<LinkedPDGNode>>>,
    pub visited: bool
}

impl From<&PDGSpecNode> for LinkedPDGNode {
    fn from(value: &PDGSpecNode) -> Self {
        LinkedPDGNode { _file: value.file.clone(), _line: value.line, name: value.name.clone(), _kind: value.kind, connections: Vec::new(), visited: false }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CFGSpecStatement {
    pub stmtRef: u32,
    #[serde(default)]
    pub predStmtRef: Option<u32>,
    #[serde(default)]
    pub trueBranch: Option<Vec<CFGSpecStatement>>,
    #[serde(default)]
    pub falseBranch: Option<Vec<CFGSpecStatement>>,
}