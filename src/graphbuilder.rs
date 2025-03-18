use std::{collections::HashMap, fs::File, io::{self, BufReader}, path::Path};
use vcd::{Command as Command, IdCode};
use anyhow::Result;

use crate::{pdg_spec::PDGSpec, Error};

pub struct GraphBuilder {
    reader: VcdReader,
    pdg: PDGSpec,
    pred_values: HashMap<IdCode, bool>,
    pred_idx_to_id: Vec<IdCode>
    // This struct should contain some kind of state.
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

impl GraphBuilder {
    pub fn new(vcd_path: impl AsRef<Path>, pdg: PDGSpec) -> Result<GraphBuilder> {
        let vcd_reader = VcdReader::new(vcd_path)?;
        Ok(GraphBuilder { reader: vcd_reader, pdg, pred_values: HashMap::new(), pred_idx_to_id: vec![] })
    }

    pub fn process(&mut self) -> Result<()> {
        self.init_predicates()?;

        let mut eof_reached = false;
        while !eof_reached {
            let (c, eof) = self.reader.read_cycle_changes()?;
            eof_reached = eof;

            println!("Activated nodes: {:?}", self.get_activated_statements(&c));
        } 

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
            activated.push(node.stmtRef);
            if let Some(pred) = node.predStmtRef {
                let pred_id = self.pred_idx_to_id[pred as usize];
                let pred_active = self.pred_values[&pred_id];
                if pred_active {
                    if let Some(t_branch) = node.trueBranch {
                        stack.extend(t_branch);
                    }
                } else {
                    if let Some(f_branch) = node.falseBranch {
                        stack.extend(f_branch);
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
        let mut hier_path = vec!["TOP", "svsimTestbech", "dut"];
        hier_path.extend(hierarchy.as_ref().split("."));
        Ok(self.header.find_var(&hier_path).ok_or(Error::VariableNotFoundError(hier_path.join(".")))?.code)
    }

    fn read_cycle_changes(&mut self) -> Result<(Vec<ValueChange>, bool)> {
        let mut changes = vec![];
        let mut rising_edge_found = false;
        let mut eof_reached = true;
        while let Some(command) = self.parser.next() {
            let command = command?;
            match command {
                Command::Timestamp(t) => {
                    // println!("Timestamp: {t}");
                    if rising_edge_found {
                        self.current_time = t + 1;
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



        Ok((changes, eof_reached))
    }
}