use std::{cell::RefCell, collections::HashMap, fs::File, io::{self, BufReader}, path::Path, rc::Rc};
use serde::Serialize;
use vcd::{Command as Command, IdCode};
use anyhow::Result;

use crate::{conversion::dpdg_make_exportable, pdg_spec::{ExportablePDG, PDGSpec, PDGSpecEdge, PDGSpecEdgeKind, PDGSpecNode, PDGSpecNodeKind}, errors::Error};

pub struct GraphBuilder {
    reader: VcdReader,
    pdg: PDGSpec,
    linked_nodes: Vec<Rc<RefCell<PDGNode>>>,
    pred_values: HashMap<IdCode, bool>,
    pred_idx_to_id: Vec<IdCode>,
    // This struct should contain some kind of state.
    dependency_state: HashMap<String, Rc<RefCell<DynPDGNode>>>
}

struct VcdReader {
    parser: vcd::Parser<io::BufReader<File>>,
    extra_scopes: Vec<String>,
    header: vcd::Header,
    clock: vcd::IdCode,
    _reset: vcd::IdCode,
    current_time: u64,
    clock_val: vcd::Value,
    changes_buffer: Vec<ValueChange>,
    probes: HashMap<IdCode, Vec<String>>,
    probe_values: HashMap<String, u64>,
    probe_change_buffer: Vec<(String, u64)>
}

#[derive(Debug, Clone, Copy)]
struct ValueChange {
    id: vcd::IdCode,
    value: vcd::Value
}

#[derive(Debug)]
struct PDGNode {
    inner: PDGSpecNode,
    provides: Vec<(Rc<RefCell<PDGNode>>, PDGSpecEdge)>,
    dependencies: Vec<(Rc<RefCell<PDGNode>>, PDGSpecEdge)>
}

#[derive(Debug, Serialize)]
pub struct DynPDGNode {
    pub inner: PDGSpecNode,
    pub timestamp: u64,
    pub dependencies: Vec<(Rc<RefCell<DynPDGNode>>, PDGSpecEdgeKind)>
}

#[derive(Debug, Clone)]
pub enum CriterionType {
    Statement(String),
    Signal(String)
}

impl GraphBuilder {
    pub fn new(vcd_path: impl AsRef<Path>, extra_scopes: Vec<String>, pdg: PDGSpec) -> Result<GraphBuilder> {
        let vcd_reader = VcdReader::new(vcd_path, extra_scopes)?;

        // Link up the nodes for easier processing
        let linked = pdg.vertices.iter().map(|v| {
            Rc::new(RefCell::new(PDGNode {inner: v.clone(), provides: vec![], dependencies: vec![] }))
        }).collect::<Vec<_>>();

        for (node_idx, node) in linked.iter().enumerate() {
            for edge in &pdg.edges {
                if edge.from == node_idx as u32 {
                    let mut node_ref = node.borrow_mut();
                    node_ref.dependencies.push((linked[edge.to as usize].clone(), edge.clone()));
                }
                if edge.to == node_idx as u32 {
                    node.borrow_mut().provides.push((linked[edge.from as usize].clone(), edge.clone()));
                }
            }
        }

        Ok(GraphBuilder { reader: vcd_reader, pdg, linked_nodes: linked, pred_values: HashMap::new(), pred_idx_to_id: vec![], dependency_state: HashMap::new() })
    }

    pub fn process(&mut self, criterion: &CriterionType, max_timesteps: Option<u64>) -> Result<ExportablePDG> {
        self.init_predicates()?;

        let mut eof_reached = false;
        let mut all_nodes = vec![];
        while !eof_reached && self.reader.current_time * 2 <= max_timesteps.unwrap_or(u64::MAX) {
            let (c, eof) = self.reader.read_cycle_changes()?;
            let corrected_timestamp = self.reader.current_time - 1; // Time starts at zero
            eof_reached = eof;
            let activated_statements = self.get_activated_statements(&c);
            let mut new_reg_providers: HashMap<String, Rc<RefCell<DynPDGNode>>> = HashMap::new();
            let mut controlflow_providers: HashMap<PDGSpecNode, Rc<RefCell<DynPDGNode>>> = HashMap::new();
            let mut new_nodes = vec![];
            for stmt in &activated_statements {
                let node = self.linked_nodes[*stmt as usize].borrow();
                // Without this fix, we get a situation where registers of timestamp x can depend on wires from timestamp x, which is clearly
                // incorrect if you operate under the assumption that on each rising edge, the registers update, THEN the wires that depend on those
                // update
                let node_timestamp = if node.inner.clocked { corrected_timestamp } else { corrected_timestamp.saturating_sub(1) };
                let dpdg_node = Rc::new(RefCell::new(DynPDGNode {inner: node.inner.clone(), timestamp: node_timestamp, dependencies: vec![]}));
                new_nodes.push((self.linked_nodes[*stmt as usize].clone(), dpdg_node.clone()));

                let conditions_satisfied = if let Some(conds) = &node.inner.condition {
                    conds.probe_name.iter().zip(&conds.probe_value).all(|(probe, required_value)| {
                        if let Some(current_probe_val) = self.reader.probe_values.get(probe) {
                            *required_value == *current_probe_val
                        } else {
                            false
                        }
                    })
                } else {
                    true
                };
                // First, update all the wires dependencies. This will determine during the dependency finding which statement will provide which
                // wire value (this is possible because we are just tracing dependencies between statements). In the same pass, we can do registers.
                // We will have to place them in a buffer, because the dependencies are delayed by one clock cycle.
                if conditions_satisfied {
                    if let Some(symb) = &node.inner.assigns_to { // Add conditions
                        if node.inner.clocked {
                            if node.inner.kind == PDGSpecNodeKind::DataDefinition {
                                // println!("Register init found");
                                // Handle register resets.
                                if corrected_timestamp == 0 {
                                    // println!("Register with reset: {:?}", node.inner.name);
                                    // dpdg_node.borrow_mut().timestamp -= 1;
                                    self.dependency_state.insert(symb.clone(), dpdg_node.clone());
                                }
                            } else {
                                new_reg_providers.insert(symb.clone(), dpdg_node.clone());
                            }
                        } else {
                            self.dependency_state.insert(symb.clone(), dpdg_node.clone());
                        }
                    }

                    if node.inner.kind == PDGSpecNodeKind::ControlFlow {
                        controlflow_providers.insert(node.inner.clone(), dpdg_node.clone());
                    }
                }
            }
            for (node, dpdg_node) in &new_nodes {
                // A statement may depend on multiple statements that provide the same symbol.
                // We only want to process the symbol once, otherwise we get duplicate dependencies.
                let mut deps_processed = vec![];
                // println!("Statement {:?}. Dependencies: {:?}", node.borrow().inner.name, node.borrow().dependencies.iter().map(|d| d.0.borrow().inner.name.clone()).collect::<Vec<_>>());
                for (dep_node, dep_edge) in &node.borrow().dependencies {
                    if let Some(ref assigns_to) = dep_node.borrow().inner.assigns_to {
                        // if node.borrow().inner.name == "connect_io.r_data" {
                        //     println!("Processing dep {:?} with edge {:?}", dep_node.borrow().inner.name, dep_edge);
                        //     println!("====> Assigns to: {:?}", assigns_to);
                        // }
                        if deps_processed.contains(assigns_to) {
                            continue;
                        }
                    }
                    let conditions_satisfied = if let Some(conds) = &dep_edge.condition {
                        conds.probe_name.iter().zip(&conds.probe_value).all(|(probe, required_value)| {
                            // println!("Probe: {}, required: {}, actual: ", probe, required_value);
                            // println!("{:?}", self.reader.probe_values);
                            if let Some(current_probe_val) = self.reader.probe_values.get(probe) {
                                *required_value == *current_probe_val
                            } else {
                                false
                            }
                        })
                    } else {
                        true
                    };

                    if conditions_satisfied {
                        match dep_edge.kind {
                            PDGSpecEdgeKind::Data | PDGSpecEdgeKind::Index  => {
                                if let Some(dep_str) = &dep_node.borrow().inner.assigns_to {
                                    if let Some(dep) = self.dependency_state.get(dep_str) {
                                        dpdg_node.borrow_mut().dependencies.push((dep.clone(), dep_edge.kind));
                                    }
                                    deps_processed.push(dep_str.clone());
                                }
                            }
                            PDGSpecEdgeKind::Conditional => {
                                if let Some(cond_dep) = controlflow_providers.get(&dep_node.borrow().inner) {
                                    dpdg_node.borrow_mut().dependencies.push((cond_dep.clone(), PDGSpecEdgeKind::Conditional));
                                }
                            }
                            _ => ()
                        }
                    }
                }
            }

            for (_,n) in new_nodes {
                all_nodes.push(n);
            }
            for (k,v) in new_reg_providers {
                self.dependency_state.insert(k, v);
            }
            // println!("{}", corrected_timestamp);
            // println!("Activated nodes: {:?}", activated_statements);

            // println!("{:#?}", self.reader.probe_values);
        }

        // println!("Full graph: {:#?}", all_nodes[all_nodes.len()-1]);
        println!("Amount of nodes: {}", all_nodes.len());

        let exported_node = all_nodes.iter()
            .filter(|n| {
                match criterion {
                    CriterionType::Statement(c) => n.borrow().inner.name.eq(c),
                    CriterionType::Signal(c) => n.borrow().inner.assigns_to.as_ref() == Some(c)
                }
            })
            .max_by_key(|n| n.borrow().timestamp)
            .ok_or(Error::StatementLookupError("Criterion not found in DPDG".into()))?;

        println!("Making pdg exportable");
        Ok(dpdg_make_exportable(exported_node.clone()))
    }

    fn init_predicates(&mut self) -> Result<()> {
        for pred in &self.pdg.predicates {
            let pred_id = self.reader.find_var(&pred.name)?;
            self.pred_values.insert(pred_id, false);
            self.pred_idx_to_id.push(pred_id);
        }

        Ok(())
    }

    fn get_activated_statements(&mut self, changes: &Vec<ValueChange>) -> Vec<u32> {
        for change in changes {
            if let Some(v) = self.pred_values.get_mut(&change.id) {
                *v = change.value == vcd::Value::V1;
            }
        }

        let mut activated = Vec::new();

        let mut stack = self.pdg.cfg.clone();
        stack.reverse();

        while let Some(node) = stack.pop() {
            activated.push(node.stmt_ref);
            if let Some(pred) = node.pred_stmt_ref {
                let pred_id = self.pred_idx_to_id[pred as usize];
                let pred_active = self.pred_values[&pred_id];
                if pred_active {
                    if let Some(t_branch) = node.true_branch {
                        stack.extend(t_branch.into_iter().rev());
                    }
                } else if let Some(f_branch) = node.false_branch {
                    stack.extend(f_branch.into_iter().rev());
                }
            }
        }

        activated
    }
}

impl VcdReader {
    fn new(vcd_path: impl AsRef<Path>, extra_scopes: Vec<String>) -> Result<Self> {
        let file = File::open(vcd_path)?;
        let reader = BufReader::new(file);
        let mut parser = vcd::Parser::new(reader);
        let header = parser.parse_header()?;
        // println!("{:#?}", header);
        let mut clock_path = extra_scopes.clone();
        clock_path.push("clock".into());

        let mut reset_path = extra_scopes.clone();
        reset_path.push("reset".into());

        let clock = header.find_var(&clock_path).ok_or(Error::ClockNotFoundError)?.code;
        let _reset = header.find_var(&reset_path).ok_or(Error::ClockNotFoundError)?.code;

        let probes = Self::find_probes(&header, &extra_scopes);
        
        Ok(VcdReader { parser, extra_scopes, header, clock, _reset, current_time: 0, clock_val: vcd::Value::X, changes_buffer: vec![], probes, probe_values: HashMap::new(), probe_change_buffer: vec![] })
    }

    fn find_probes(header: &vcd::Header, root_scope: &[String]) -> HashMap<IdCode, Vec<String>> {
        let mut probes = HashMap::new();
        if let Some(dut) = header.find_scope(root_scope) {
            let mut stack = vec![];
            stack.extend_from_slice(&dut.items.iter().map(|i| ("".to_string(), i)).collect::<Vec<_>>());
            while let Some((prefix, item)) = stack.pop() {
                match item {
                    vcd::ScopeItem::Scope(scope) => {
                        stack.extend_from_slice(&scope.items.iter().map(|i| (prefix.to_string() + &scope.identifier, i)).collect::<Vec<_>>());
                    }
                    vcd::ScopeItem::Var(var) => {
                        // Probes may have the same IdCode if they are driven by the same value.
                        // We need to check if it exists and update the vector if it does.
                        if var.reference.starts_with("probe_") {
                            let probe_path = if prefix.is_empty() {
                                var.reference.clone()
                            } else {
                                prefix.clone() + "." + &var.reference
                            };
                            probes.entry(var.code).and_modify(|e: &mut Vec<String>| e.push(probe_path.clone())).or_insert(vec![probe_path]);
                        }
                    }
                    _ => ()
                }
            }
        }

        probes
    }

    fn find_var(&self, hierarchy: impl AsRef<str>) -> Result<IdCode> {
        let mut hier_path = self.extra_scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        hier_path.extend(hierarchy.as_ref().split("."));
        Ok(self.header.find_var(&hier_path).ok_or(Error::VariableNotFoundError(hier_path.join(".")))?.code)
    }

    fn read_cycle_changes(&mut self) -> Result<(Vec<ValueChange>, bool)> {
        let mut changes = vec![];
        let mut rising_edge_found = false;
        let mut eof_reached = true;
        let last_time = self.current_time;
        for command in self.parser.by_ref() {
            let command = command?;
            match command {
                Command::Timestamp(_t) => {
                    // println!("Timestamp: {t}");
                    // The events that are recorded at the same step as a rising edge take place *after* the clock edge.
                    // Therefore, they should be processed at the next time step.
                    if rising_edge_found {
                        self.current_time += 1;
                        eof_reached = false;
                        break;
                    } else {
                        changes.append(&mut self.changes_buffer);
                        for change in &self.probe_change_buffer {
                            self.probe_values.insert(change.0.clone(), change.1);
                        }
                        self.probe_change_buffer.clear();
                    }
                }
                Command::ChangeScalar(i, v) if i == self.clock => {
                    if self.clock_val == vcd::Value::V0 && v == vcd::Value::V1 {
                        // println!("Rising edge");
                        rising_edge_found = true;
                    }
                    self.clock_val = v;
                }
                Command::ChangeScalar(i, v) => {
                    // println!("Change in {:?}: {v}", i);
                    if let Some(probes) = self.probes.get(&i) {
                        for probe in probes {
                            let unsigned_v = match v {
                                vcd::Value::V1 => 1,
                                _ => 0
                            };
                            self.probe_change_buffer.push((probe.clone(), unsigned_v));
                        }
                    } else {
                        self.changes_buffer.push(ValueChange { id: i, value: v });
                    }
                }
                Command::ChangeVector(i, v) => {
                    if let Some(probes) = self.probes.get(&i) {
                        for probe in probes {
                            self.probe_change_buffer.push((probe.clone(), bitvector_to_unsigned(&v)));
                        }
                    }
                    // println!("Change in vector: {:?}", i);
                }
                _ => ()
            }
        }
        if last_time == self.current_time {
            self.current_time += 1;
        }

        Ok((changes, eof_reached))
    }
}

fn bitvector_to_unsigned(input_vec: &vcd::Vector) -> u64 {
    let mut val = 0;
    let mut bitval = 1;
    // Workaround because the VCD crate does not allow for direct reversed iterator.
    let mut rev_bits = input_vec.iter().collect::<Vec<_>>();
    rev_bits.reverse();
    for input in rev_bits {
        if input == vcd::Value::V1 {
            val += bitval;
        }
        bitval <<= 1;
    }
    val
}