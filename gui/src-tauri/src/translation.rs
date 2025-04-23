#[derive(Debug, Clone)]
pub struct TranslationResult {
    pub tpe: Option<String>,
    pub value: String
}

#[derive(Clone, Copy, Debug)]
pub enum TranslationStrategy {
    /// Tries to detect the type based on the source language type
    Auto,
    /// Interprets everything as a UInt
    UInt,
    /// Does not perform any translation
    None
}

pub fn interpret_tywaves_value(val: &String, stategy: TranslationStrategy) -> TranslationResult {
    let parts = val.split(" ").collect::<Vec<_>>();
    let tpe = if parts.len() > 1 {
        Some(parts[0].to_string())
    } else { None };

    let value_part = if tpe.is_some() { parts[1].to_string() } else { parts[0].to_string() };

    let value = match stategy {
        TranslationStrategy::Auto => auto_translate(value_part, &tpe),
        TranslationStrategy::UInt => translate_as_uint(value_part),
        TranslationStrategy::None => value_part
    };

    TranslationResult { tpe, value }
}

fn auto_translate(bitstring: String, tpe: &Option<String>) -> String {
    if let Some(tpe) = tpe {
        if tpe.contains("UInt") {
            translate_as_uint(bitstring)
        } else if tpe.contains("SInt") {
            translate_as_sint(bitstring)
        } else if tpe.contains("Bool") {
            translate_as_bool(bitstring)
        } else if tpe.contains("logic") {
            translate_as_bool(bitstring)
        } else {
            bitstring
        }
    } else {
        bitstring
    }
}

fn translate_as_uint(bitstring: String) -> String {
    let mut val: u128 = 0;
    let mut bitval = 1;

    for ch in bitstring.chars().rev() {
        match ch {
            '1' => val += bitval,
            '0' => (),
            _ => return "UDF".into() // Undefined for inputs such as xx
        }
        bitval <<= 1;
    }

    val.to_string()
}

fn translate_as_sint(bitstring: String) -> String {
    if bitstring.is_empty() {
        return "UDF".into();
    }

    let mut val: i128 = 0;
    let chars: Vec<char> = bitstring.chars().collect();
    let msb_index = chars.len() - 1;

    // Process all bits except MSB first (LSB to MSB-1)
    for (i, &ch) in chars.iter().rev().enumerate() {
        if i == msb_index {
            continue;
        }
        match ch {
            '1' => val += 1 << i,
            '0' => (),
            _ => return "UDF".into(),
        }
    }

    // Handle MSB (sign bit)
    match chars[0] {
        '1' => val -= 1 << msb_index,
        '0' => (),
        _ => return "UDF".into(),
    }

    val.to_string()
}

fn translate_as_bool(bitstring: String) -> String {
    match bitstring.as_str() {
        "1" => "true".into(),
        "0" => "false".into(),
        _ => "UDF".into()  // Undefined for any other input
    }
}