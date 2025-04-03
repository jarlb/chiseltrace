/*
    Note: this file contains mostly copied (slightly modified) code from the tywaves translator in the surfer-tywaves repository 
*/
use std::path::Path;

use tywaves_rs::{hgldd, tyvcd::{builder::{GenericBuilder, TyVcdBuilder}, spec::{Variable, VariableKind}, trace_pointer::TraceFinder}};
use anyhow::Result;

use crate::errors::Error;



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
    kind: ValueKind
}

#[derive(Debug)]
pub struct SubFieldTranslationResult {
    name: String,
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
            .map_err(|e| Error::from(e))?;
        let mut builder = TyVcdBuilder::init(hgldd)
            .with_extra_artifact_scopes(extra_scopes, top_module);
        let _res_build = builder.build().map_err(|e| Error::from(e))?;
        Ok(Self { builder, top_module: top_module.clone() })
    }

    pub fn vcd_rewrite(&self, vcd_path: &Path) -> Result<String> {
        let tywaves_scopes = &self.builder.get_ref().unwrap().scopes;
        // Get the list of scopes
        let scopes_def_list = tywaves_scopes
            .into_iter()
            .map(|(_, v)| (v.read().unwrap().clone()))
            .collect();

        let mut vcd_rewriter = tywaves_rs::vcd_rewrite::VcdRewriter::new(
            vcd_path,
            scopes_def_list,
            format!("{}.vcd", self.top_module),
        )
        .map_err(|e| Error::from(e))?;
        
        vcd_rewriter
            .rewrite()
            .map_err(|e| Error::from(e))?;
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
                    // TODO: Fix this
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
        let render_fn = |num_bits: u64, raw_val_vcd: &str| {
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
                        kind: ValueKind::Normal,
                    };
                    subfields.push(SubFieldTranslationResult {
                        name: i.to_string(),
                        result: subfield,
                    });
                }

                let subfields = match self.convert_kind2info(&variable.kind) {
                    VariableInfo::Bool => vec![],
                    _ => subfields,
                };

                // let kind = if subfields.len() > 1 {
                //     ValueKind::Custom(Color32::KHAKI)
                // } else {
                //     ValueKind::Normal
                // }; // TODO!: change this to use the correct value
                let kind = ValueKind::Normal;

                TranslationResult {
                    val: val_repr,
                    subfields,
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
                        result: self.translate_variable(field, _val)?,
                    });
                }

                TranslationResult {
                    val: val_repr,
                    subfields,
                    // kind: ValueKind::Custom(Color32::BLUE),
                    kind: ValueKind::Normal,
                }
            }
            _ => TranslationResult {
                val: raw_val_vcd.to_string(),
                subfields: vec![],
                kind: ValueKind::Undef,
            },
        };

        Ok(result)
    }
}