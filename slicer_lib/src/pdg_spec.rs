use std::{cell::RefCell, rc::Rc};

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct PDGSpec {
    pub vertices: Vec<PDGSpecNode>,
    pub edges: Vec<PDGSpecEdge>,
    pub predicates: Vec<PDGSpecNode>,
    pub cfg: Vec<CFGSpecStatement>
}

impl PDGSpec {
    pub fn _empty() -> Self {
        PDGSpec { vertices: vec![], edges: vec![], predicates: vec![], cfg: vec![] }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PDGSpecNode {
    pub file: String,
    pub line: u32,
    pub char: u32,
    pub name: String,
    pub kind: PDGSpecNodeKind,
    pub clocked: bool,
    pub related_signal: Option<PDGSpecRelatedSignal>,
    pub assigns_to: Option<String>,
    pub is_chisel_statement: bool,
    pub condition: Option<PDGSpecCondition>,
    #[serde(default)]
    pub assign_delay: u32
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Hash, Eq)]
pub enum PDGSpecNodeKind {
    Definition,
    DataDefinition,
    IO,
    Connection,
    ControlFlow,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PDGSpecRelatedSignal {
    pub signal_path: String,
    pub field_path: String
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PDGSpecEdge {
    pub from: u32,
    pub to: u32,
    pub kind: PDGSpecEdgeKind,
    pub clocked: bool,
    pub condition: Option<PDGSpecCondition>
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PDGSpecEdgeKind {
    Data,
    Conditional,
    Declaration,
    Index
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PDGSpecCondition {
    pub probe_name: Vec<String>,
    pub probe_value: Vec<u64>
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
#[serde(rename_all = "camelCase")]
pub struct CFGSpecStatement {
    pub stmt_ref: u32,
    #[serde(default)]
    pub pred_stmt_ref: Option<u32>,
    #[serde(default)]
    pub true_branch: Option<Vec<CFGSpecStatement>>,
    #[serde(default)]
    pub false_branch: Option<Vec<CFGSpecStatement>>,
}

/// A format of the PDG that allows for storage and export of additional information.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExportablePDG {
    pub vertices: Vec<ExportablePDGNode>,
    pub edges: Vec<ExportablePDGEdge>
}

impl ExportablePDG {
    pub fn empty() -> Self {
        ExportablePDG { vertices: vec![], edges: vec![] }
    }
}

impl From<PDGSpec> for ExportablePDG {
    fn from(value: PDGSpec) -> Self {
        ExportablePDG { vertices: value.vertices.into_iter().map(|v| v.into()).collect(),
            edges: value.edges.into_iter().map(|e| e.into()).collect() }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExportableSlice {
    pub statements: Vec<ExportableSliceStatement>
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct ExportableSliceStatement {
    pub file: String,
    pub line: u32,
    pub char: u32
}

impl From<ExportablePDGNode> for ExportableSliceStatement {
    fn from(value: ExportablePDGNode) -> Self {
        ExportableSliceStatement { file: value.file, line: value.line, char: value.char }
    }
}

impl From<PDGSpecNode> for ExportableSliceStatement {
    fn from(value: PDGSpecNode) -> Self {
        ExportableSliceStatement { file: value.file, line: value.line, char: value.char }
    }
}

impl From<Rc<PDGSpecNode>> for ExportableSliceStatement {
    fn from(value: Rc<PDGSpecNode>) -> Self {
        ExportableSliceStatement { file: value.file.clone(), line: value.line, char: value.char }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExportablePDGNode {
    pub file: String,
    pub line: u32,
    pub char: u32,
    pub name: String,
    pub kind: PDGSpecNodeKind,
    pub clocked: bool,
    pub related_signal: Option<PDGSpecRelatedSignal>,
    pub sim_data: Option<String>,
    pub timestamp: i64,
    pub is_chisel_assignment: bool
}

impl From<PDGSpecNode> for ExportablePDGNode {
    fn from(value: PDGSpecNode) -> Self {
        ExportablePDGNode { file: value.file, line: value.line, char: value.char, name: value.name, kind: value.kind,
            clocked: value.clocked, related_signal: value.related_signal, sim_data: None,
            is_chisel_assignment: value.is_chisel_statement, timestamp: 0
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct ExportablePDGEdge {
    pub from: u32,
    pub to: u32,
    pub kind: PDGSpecEdgeKind,
    pub clocked: bool
}

impl From<PDGSpecEdge> for ExportablePDGEdge {
    fn from(value: PDGSpecEdge) -> Self {
        ExportablePDGEdge { from: value.from, to: value.to, kind: value.kind, clocked: value.clocked }
    }
}