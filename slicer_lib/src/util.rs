use crate::graphbuilder::CriterionType;

pub fn parse_criterion(s: &str) -> Result<CriterionType, String> {
    let (kind, value) = s.split_once(':')
        .ok_or("Expected 'type:value' format")?;
    match kind.to_lowercase().as_str() {
        "statement" => Ok(CriterionType::Statement(value.into())),
        "signal" => Ok(CriterionType::Signal(value.into())),
        _ => Err(format!("Unknown criterion type '{}'", kind)),
    }
}