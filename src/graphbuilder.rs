use std::{cell::RefCell, collections::HashMap, fs::File, io::{self, BufReader}, path::Path, rc::Rc};
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
    changes_buffer: Vec<ValueChange>
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

#[derive(Debug)]
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
                // First, update all the wires dependencies. This will determine during the dependency finding which statement will provide which
                // wire value (this is possible because we are just tracing dependencies between statements). In the same pass, we can do registers.
                // We will have to place them in a buffer, because the dependencies are delayed by one clock cycle.
                if let Some(symb) = &node.inner.assigns_to { // Add conditions
                    if node.is_register {
                        new_reg_providers.insert(symb.clone(), dpdg_node.clone());
                    } else {
                        self.dependency_state.insert(symb.clone(), dpdg_node.clone());
                    }
                }

                if node.inner.kind == PDGSpecNodeKind::ControlFlow {
                    controlflow_providers.insert(node.inner.clone(), dpdg_node.clone());
                }
            }
            for (node, dpdg_node) in &new_nodes {
                for (dep_node, dep_edge) in &node.borrow().dependencies {
                    match dep_edge.kind {
                        PDGSpecEdgeKind::Data => {
                            if let Some(dep_str) = &dep_node.borrow().inner.assigns_to {
                                if let Some(dep) = self.dependency_state.get(dep_str) {
                                    dpdg_node.borrow_mut().dependencies.push((dep.clone(), PDGSpecEdgeKind::Data));
                                }
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

            for (_,n) in new_nodes {
                all_nodes.push(n);
            }
            for (k,v) in new_reg_providers {
                self.dependency_state.insert(k, v);
            }
            println!("{}", self.reader.current_time);
            println!("Activated nodes: {:?}", activated_statements);
        }

        println!("Full graph: {:#?}", all_nodes[all_nodes.len()-1]);

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
        
        Ok(VcdReader { parser, header, clock, reset, current_time: 0, clock_val: vcd::Value::X, changes_buffer: vec![] })
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