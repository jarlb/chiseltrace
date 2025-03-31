use std::{cell::RefCell, collections::HashMap, fs::File, io::{self, BufReader, BufWriter}, path::Path, rc::Rc};
use serde::Serialize;
use vcd::{Command as Command, IdCode};
use anyhow::Result;

use crate::{pdg_spec::{PDGSpec, PDGSpecEdge, PDGSpecEdgeKind, PDGSpecNode, PDGSpecNodeKind}, Error};

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
    header: vcd::Header,
    clock: vcd::IdCode,
    reset: vcd::IdCode,
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
    dependencies: Vec<(Rc<RefCell<PDGNode>>, PDGSpecEdge)>,
    is_register: bool
}

#[derive(Debug, Serialize)]
struct DynPDGNode {
    inner: PDGSpecNode,
    timestamp: u64,
    dependencies: Vec<(Rc<RefCell<DynPDGNode>>, PDGSpecEdgeKind)>
}

impl GraphBuilder {
    pub fn new(vcd_path: impl AsRef<Path>, pdg: PDGSpec) -> Result<GraphBuilder> {
        let vcd_reader = VcdReader::new(vcd_path)?;

        // Link up the nodes for easier processing
        let linked = pdg.vertices.iter().map(|v| {
            Rc::new(RefCell::new(PDGNode {inner: v.clone(), provides: vec![], dependencies: vec![], is_register: false }))
        }).collect::<Vec<_>>();

        for (node_idx, node) in linked.iter().enumerate() {
            for edge in &pdg.edges {
                if edge.from == node_idx as u32 {
                    let mut node_ref = node.borrow_mut();
                    node_ref.dependencies.push((linked[edge.to as usize].clone(), edge.clone()));
                    if edge.clocked {
                        node_ref.is_register = true;
                    }
                }
                if edge.to == node_idx as u32 {
                    node.borrow_mut().provides.push((linked[edge.from as usize].clone(), edge.clone()));
                }
            }
        }

        println!("Node 21 deps: {:?}", linked[21].borrow().dependencies.iter().map(|d| d.0.borrow().inner.name.clone()).collect::<Vec<_>>());

        Ok(GraphBuilder { reader: vcd_reader, pdg, linked_nodes: linked, pred_values: HashMap::new(), pred_idx_to_id: vec![], dependency_state: HashMap::new() })
    }

    pub fn process(&mut self) -> Result<()> {
        self.init_predicates()?;

        let mut eof_reached = false;
        let mut all_nodes = vec![];
        while !eof_reached {
            let (c, eof) = self.reader.read_cycle_changes()?;
            eof_reached = eof;
            let activated_statements = self.get_activated_statements(&c);
            let mut new_reg_providers: HashMap<String, Rc<RefCell<DynPDGNode>>> = HashMap::new();
            let mut controlflow_providers: HashMap<PDGSpecNode, Rc<RefCell<DynPDGNode>>> = HashMap::new();
            let mut new_nodes = vec![];
            for stmt in &activated_statements {
                let node = self.linked_nodes[*stmt as usize].borrow();
                let dpdg_node = Rc::new(RefCell::new(DynPDGNode {inner: node.inner.clone(), timestamp: self.reader.current_time, dependencies: vec![]}));
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
                        if node.is_register {
                            if node.inner.kind == PDGSpecNodeKind::DataDefinition {
                                println!("Register init found");
                                // Handle register resets.
                                if self.reader.current_time == 1 {
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
                            PDGSpecEdgeKind::Data => {
                                if let Some(dep_str) = &dep_node.borrow().inner.assigns_to {
                                    if let Some(dep) = self.dependency_state.get(dep_str) {
                                        dpdg_node.borrow_mut().dependencies.push((dep.clone(), PDGSpecEdgeKind::Data));
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
            println!("{}", self.reader.current_time);
            println!("Activated nodes: {:?}", activated_statements);

            // println!("{:#?}", self.reader.probe_values);
        }

        // println!("Full graph: {:#?}", all_nodes[all_nodes.len()-1]);
        println!("Amount of nodes: {}", all_nodes.len());

        let f = File::create("dynpdg.json")?;
        let writer = BufWriter::new(f);
        serde_json::to_writer_pretty(writer, &all_nodes[all_nodes.len()-1])?;

        Ok(())
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
                } else {
                    if let Some(f_branch) = node.false_branch {
                        stack.extend(f_branch.into_iter().rev());
                    }
                }
            }
        }

        activated
    }

    pub fn read_init(&mut self) -> Result<()> {
        println!("{:#?}", self.reader.read_cycle_changes()?.0);
        Ok(())
    }

    pub fn read_rest(&mut self) -> Result<()> {
        let mut eof_reached = false;
        while !eof_reached {
            println!("Timestep: {:?}", self.reader.current_time);
            let (c, eof) = self.reader.read_cycle_changes()?;
            println!("{:#?}", c);
            eof_reached = eof;
        }   

        Ok(())
    }
}

impl VcdReader {
    fn new(vcd_path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(vcd_path)?;
        let reader = BufReader::new(file);
        let mut parser = vcd::Parser::new(reader);
        let header = parser.parse_header()?;
        // println!("{:#?}", header);
        let clock = header.find_var(&["TOP", "svsimTestbench", "clock"]).ok_or(Error::ClockNotFoundError)?.code;
        let reset = header.find_var(&["TOP", "svsimTestbench", "reset"]).ok_or(Error::ClockNotFoundError)?.code;

        let probes = Self::find_probes(&header);
        
        Ok(VcdReader { parser, header, clock, reset, current_time: 0, clock_val: vcd::Value::X, changes_buffer: vec![], probes, probe_values: HashMap::new(), probe_change_buffer: vec![] })
    }

    fn find_probes(header: &vcd::Header) -> HashMap<IdCode, Vec<String>> {
        let mut probes = HashMap::new();
        if let Some(dut) = header.find_scope(&["TOP", "svsimTestbench", "dut"]) {
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
                            probes.entry(var.code).and_modify(|e: &mut Vec<String>| e.push(prefix.clone() + "." + &var.reference)).or_insert(vec![prefix + "." + &var.reference]);
                        }
                    }
                    _ => ()
                }
            }
        }

        probes
    }

    fn find_var(&self, hierarchy: impl AsRef<str>) -> Result<IdCode> {
        let mut hier_path = vec!["TOP", "svsimTestbench", "dut"];
        hier_path.extend(hierarchy.as_ref().split("."));
        Ok(self.header.find_var(&hier_path).ok_or(Error::VariableNotFoundError(hier_path.join(".")))?.code)
    }

    fn read_cycle_changes(&mut self) -> Result<(Vec<ValueChange>, bool)> {
        let mut changes = vec![];
        let mut rising_edge_found = false;
        let mut eof_reached = true;
        let last_time = self.current_time;
        while let Some(command) = self.parser.next() {
            let command = command?;
            match command {
                Command::Timestamp(t) => {
                    // println!("Timestamp: {t}");
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
                    self.changes_buffer.push(ValueChange { id: i, value: v });
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