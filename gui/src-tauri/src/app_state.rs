use std::{collections::HashMap, path::PathBuf};

use program_slicer_lib::{graphbuilder::CriterionType, pdg_spec::ExportablePDG};

pub struct AppState {
    pub pdg_config: Option<PDGConfig>,
    pub graph: Option<ViewableGraph>
}

impl AppState {
    pub fn new() -> Self {
        AppState { pdg_config: None, graph: None }
    }
}

#[derive(Debug, Clone)]
pub struct PDGConfig {
    pub criterion: CriterionType,
    pub pdg_path: PathBuf,
    pub vcd_path: PathBuf,
    pub hgldd_path: PathBuf,
    pub top_module: String,
    pub extra_scopes: Vec<String>,
    pub max_timesteps: Option<u64>,
    pub data_only: bool
}

#[derive(Debug, Clone)]
pub struct ViewableGraph {
    pub dpdg: ExportablePDG,
    pub time_to_nodes: HashMap<i64, Vec<usize>>,
    pub dep_to_edges: HashMap<u32, Vec<usize>>,
    pub prov_to_edges: HashMap<u32, Vec<usize>>,
    pub n_timestamps: u64,
    pub source_files: HashMap<String, Vec<String>>
}
