/*
    Note: this file contains mostly copied (slightly modified) code from the tywaves translator in the surfer-tywaves repository 
*/
use std::{collections::{HashMap, VecDeque}, fs::File, io::BufReader, path::Path};

use tywaves_rs::{hgldd, tyvcd::{builder::{GenericBuilder, TyVcdBuilder}, spec::{Variable, VariableKind}, trace_pointer::TraceFinder}};
use anyhow::Result;
use vcd::{Command, IdCode};

use crate::{errors::Error, pdg_spec::{ExportablePDG, ExportablePDGNode}};

pub struct TywavesInterface {
    builder: TyVcdBuilder<hgldd::spec::Hgldd>,
    top_module: String
}

// Essentially the Surfer value kinds, but with some types removed, such as high impedance
#[derive(Clone, PartialEq, Copy, Debug)]
pub enum ValueKind {
    Normal,
    Undef,
    DontCare
}

// Also copied from surfer
#[derive(Clone, Debug, Default)]
pub enum VariableInfo {
    Compound {
        subfields: Vec<(String, VariableInfo)>,
    },
    Bits,
    Bool,
    Clock,
    #[default]
    String,
    Real,
}

#[derive(Debug)]
pub struct TranslationResult {
    val: String,
    subfields: Vec<SubFieldTranslationResult>,
    is_ground: bool,
    kind: ValueKind
}

#[derive(Debug)]
pub struct SubFieldTranslationResult {
    name: String,
    identifier: String,
    type_info: String,
    result: TranslationResult
}

#[inline]
fn create_translation_result_name(variable: &Variable) -> String {
    format!("{}: {}", variable.high_level_info.type_name, variable.name)
}

/// An interface to Tywaves that is based on the one available in the surfer-tywaves project
impl TywavesInterface {
    pub fn new(hgldd_dir: &Path, extra_scopes: Vec<String>, top_module: &String) -> Result<Self> {
        let hgldd = hgldd::reader::parse_hgldd_dir(hgldd_dir)
            .map_err(Error::from)?;
        let mut builder = TyVcdBuilder::init(hgldd)
            .with_extra_artifact_scopes(extra_scopes, top_module);
        builder.build().map_err(Error::from)?;
        Ok(Self { builder, top_module: top_module.clone() })
    }

    pub fn vcd_rewrite(&self, vcd_path: &Path) -> Result<String> {
        let tywaves_scopes = &self.builder.get_ref().unwrap().scopes;
        // Get the list of scopes
        let scopes_def_list = tywaves_scopes
            .iter()
            .map(|(_, v)| (v.read().unwrap().clone()))
            .collect();

        let mut vcd_rewriter = tywaves_rs::vcd_rewrite::VcdRewriter::new(
            vcd_path,
            scopes_def_list,
            format!("{}.vcd", self.top_module),
        )
        .map_err(Error::from)?;
        
        vcd_rewriter
            .rewrite()
            .map_err(Error::from)?;
        Ok(vcd_rewriter.get_final_file().clone())
    }

    pub fn find_signal(&self, path: &[String]) -> Result<Variable> {
        let trace_getter = self.builder.get_ref().unwrap().find_trace(path).ok_or(Error::TywavesSignalNotFound)?;
        let binding = trace_getter.read().unwrap();
        let signal = binding.as_any().downcast_ref::<Variable>().ok_or(Error::TywavesDowncastFailed)?;
        Ok(signal.clone())
    }

    /// Extract the value of a subfield from a raw value.
    /// Return the value of the subfield and the rest of the raw value.
    fn get_sub_raw_val<'a>(
        &self,
        subfield_kind: &VariableKind,
        raw_val_vcd: &'a str,
    ) -> (&'a str, &'a str) {
        // Get size of real type
        let size = subfield_kind.find_width() as usize;
        if raw_val_vcd.len() < size {
            return ("0", raw_val_vcd);
        }
        // Return the value of the subfield and the rest of the raw value
        (&raw_val_vcd[..size], &raw_val_vcd[size..])
    }

    fn convert_kind2info(&self, real_type: &VariableKind) -> VariableInfo {
        match real_type {
            VariableKind::Ground(width) => {
                if *width == 1 {
                    VariableInfo::Bool
                } else {
                    // TODO: Change this to bits
                    let mut subfields = vec![];
                    for i in 0..*width {
                        subfields.push((i.to_string(), VariableInfo::Bool));
                    }
                    VariableInfo::Compound { subfields }
                }
            }
            VariableKind::Struct { fields } | VariableKind::Vector { fields } => {
                VariableInfo::Compound {
                    subfields: fields
                        .iter()
                        .map(|f| {
                            (
                                create_translation_result_name(f),
                                self.convert_kind2info(&f.kind),
                            )
                        })
                        .collect(),
                }
            }
            _ => VariableInfo::String,
        }
    }

    pub fn translate_variable(
        &self,
        variable: &Variable,
        raw_val_vcd: &str,
    ) -> Result<TranslationResult> {
        // Create the value representation
        let render_fn = |_num_bits: u64, raw_val_vcd: &str| {
            raw_val_vcd.to_string()
        };

        let val_repr = variable.create_val_repr(raw_val_vcd, &render_fn);

        // Create a result based on the kind of the variable
        let result = match &variable.kind {
            // Create a bool if the variable is a ground with width 1
            // otherwise a bit vector
            VariableKind::Ground(width) => {
                let mut subfields = vec![];
                for i in 0..*width as usize {
                    let subfield = TranslationResult {
                        val: raw_val_vcd.chars().nth(i).unwrap().to_string(),
                        subfields: vec![],
                        is_ground: true,
                        kind: ValueKind::Normal,
                    };
                    subfields.push(SubFieldTranslationResult {
                        name: i.to_string(),
                        identifier: i.to_string(),
                        type_info: "UInt<1>".into(),
                        result: subfield,
                    });
                }

                let subfields = match self.convert_kind2info(&variable.kind) {
                    VariableInfo::Bool => vec![],
                    _ => subfields,
                };

                let kind = ValueKind::Normal;

                TranslationResult {
                    val: val_repr,
                    subfields,
                    is_ground: true,
                    kind,
                }
            }
            // Create a compound value if the variable is a struct or a vector
            VariableKind::Struct { fields } | VariableKind::Vector { fields } => {
                // Collect the subfields of the bundle
                let mut subfields = vec![];

                let mut _raw_val_vcd = raw_val_vcd;
                let mut _val = "0";
                for field in fields {
                    // Get the value of the subfield
                    (_val, _raw_val_vcd) = self.get_sub_raw_val(&field.kind, _raw_val_vcd);

                    subfields.push(SubFieldTranslationResult {
                        name: create_translation_result_name(field),
                        identifier: field.name.clone(),
                        type_info: field.high_level_info.type_name.clone(),
                        result: self.translate_variable(field, _val)?,
                    });
                }

                TranslationResult {
                    val: val_repr,
                    subfields,
                    is_ground: false,
                    kind: ValueKind::Normal,
                }
            }
            _ => TranslationResult {
                val: raw_val_vcd.to_string(),
                subfields: vec![],
                is_ground: true,
                kind: ValueKind::Undef,
            },
        };

        Ok(result)
    }

    // To inject simulation data into the graph:
    // We essentially want to associate simulation values with nodes.
    // Due to the way the timestamps are set up, it should be possible to just grab the values from the 
    // timestamps of the nodes. We do need a cache for if the values don't change
    // 1) Read in a cycle of changes, update the cache
    // 2) For each node, look up the base signal path in the VCD, then do a tywaves lookup using this value and
    // select based on the field path
    // 3) Add the information to the node

    pub fn inject_sim_data(&self, pdg: &mut ExportablePDG, vcd_path: impl AsRef<Path>) -> Result<()> {
        let file = File::open(vcd_path)?;
        let reader = BufReader::new(file);
        let mut parser = vcd::Parser::new(reader);
        let header = parser.parse_header()?;

        let signal_mapping = build_signal_map(&header);

        let mut node_map: HashMap<i64, Vec<&mut ExportablePDGNode>> = HashMap::new();
        for node in &mut pdg.vertices {
            node_map.entry(node.timestamp).or_default().push(node);
        }

        let top_path: Vec<String> = vec!["TOP".into(), "svsimTestbench".into(), "dut".into()];

        let clock = header.find_var(&["TOP", "svsimTestbench", "dut", "clock"]).ok_or(Error::ClockNotFoundError)?.code;
        
        // The rewritten VCD is a bit weird. It's best to squash all the changes (keep only the last one) for each timestep
        // (needs hashmap). Then on the timestamp after a clock cycle, update the global hashmap and add the values to the nodes

        let mut values_cache: HashMap<String, String> = HashMap::new();
        let mut rising_edge_found = false;
        let mut current_timestamp: i64 = -1;
        let mut clock_val = vcd::Value::V0;
        let mut cycle_changes: HashMap<IdCode, vcd::Vector> = HashMap::new();
        for command in parser {
            let command = command?;
            match command {
                Command::Timestamp(t) => {
                    // println!("Timestamp: {t}, current time: {current_timestamp}");
                    // Update the global hashmap with the changes
                    if rising_edge_found {
                        if current_timestamp < 0 {
                            current_timestamp = 0;
                        }
                        rising_edge_found = false;
                        for (k,v) in &cycle_changes {
                            let signals = &signal_mapping[k];
                            for signal in signals {
                                values_cache.insert(signal.clone(), v.to_string());
                            }
                        }
                        if let Some(nodes) = node_map.get_mut(&current_timestamp) {
                            for node in nodes {
                                if let Some(related_signal) = &node.related_signal {
                                    let mut hier_path = top_path.clone();
                                    hier_path.extend_from_slice(&related_signal.signal_path.split(".").map(|s| s.to_string()).collect::<Vec<_>>());
                                    let ty_var = self.find_signal(&hier_path).ok();
                                    // println!("{:#?}", ty_var);
                                    if let (Some(value), Some(tywaves_signal)) = (values_cache.get(&related_signal.signal_path), ty_var)  {
                                        let translated_var = self.translate_variable(&tywaves_signal, value)?;
                                        node.sim_data = find_ground_field(&translated_var, &related_signal.field_path);
                                    }
                                }
                            }
                        }

                        current_timestamp += 1;
                        cycle_changes.clear();
                    } else {
                        // We need to determine the exact signal changes that occurred on the falling edge and put
                        // println!("{current_timestamp}");
                        // println!("{:#?}", cycle_changes);
                        for (k,v) in &cycle_changes {
                            let signals = &signal_mapping[k];
                            for signal in signals {
                                values_cache.insert(signal.clone(), v.to_string());
                            }
                        }
                        let time = if current_timestamp == -1 {
                            current_timestamp
                        } else {
                            current_timestamp.saturating_sub(1)
                        };
                        if let Some(nodes) = node_map.get_mut(&time) {
                            for node in nodes {
                                if let Some(related_signal) = &node.related_signal {
                                    let mut hier_path = top_path.clone();
                                    hier_path.extend_from_slice(&related_signal.signal_path.split(".").map(|s| s.to_string()).collect::<Vec<_>>());
                                    let ty_var = self.find_signal(&hier_path).ok();
                                    // println!("{:#?}", ty_var);
                                    if let (Some(value), Some(tywaves_signal)) = (values_cache.get(&related_signal.signal_path), ty_var)  {
                                        let translated_var = self.translate_variable(&tywaves_signal, value)?;
                                        node.sim_data = find_ground_field(&translated_var, &related_signal.field_path);
                                    }
                                }
                            }
                        }
                        cycle_changes.clear();
                    }
                }
                Command::ChangeVector(i, v) if i == clock => {
                    let new_clock_val  = v.get(0).unwrap();
                    if clock_val == vcd::Value::V0 && new_clock_val == vcd::Value::V1 {
                        // println!("Rising edge");
                        rising_edge_found = true;
                    }
                    clock_val = new_clock_val;
                }
                Command::ChangeVector(i, v) => {
                    // println!("Change in {:?}: {v}", i);
                    cycle_changes.insert(i, v);
                    // if let Some(probes) = self.probes.get(&i) {
                    //     for probe in probes {
                    //         self.probe_change_buffer.push((probe.clone(), bitvector_to_unsigned(&v)));
                    //     }
                    // }
                }
                // Everything is vectorized by the VCD rewriter, so no scalar changes.
                _ => ()
            }
        }

        Ok(())
    }
}

/// Build a map of IdCode -> Hierarchical signal name
fn build_signal_map(header: &vcd::Header) -> HashMap<IdCode, Vec<String>> {
    let mut signals = HashMap::new();
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
                    let name = if prefix.is_empty() { var.reference.clone() } else { prefix.clone() + "." + &var.reference };
                    signals.entry(var.code).and_modify(|e: &mut Vec<String>| e.push(name.clone())).or_insert(vec![name]);
                }
                _ => ()
            }
        }
    }

    signals
}

fn find_ground_field(var: &TranslationResult, path: &String) -> Option<String> {
    if var.is_ground {
        return Some(var.val.clone())
    }
    let mut path_parts = path.split(".").collect::<VecDeque<_>>();
    let mut current_result = var;
    let mut last_type = None;
    while let Some(field) = path_parts.pop_front() {
        let Some(subfield) = current_result.subfields.iter().find(|s| s.identifier == field) else {
            return None
        };
        current_result = &subfield.result;
        last_type = Some(&subfield.type_info);
    }
    let prefix = if let Some(tpe) = last_type {format!("{} ", tpe)} else {"".into()};
    Some(prefix + &current_result.val)
}