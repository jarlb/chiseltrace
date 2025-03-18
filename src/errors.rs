use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid slicing statement: {0}")]
    StatementLookupError(String),
    #[error("Clock signal not found")]
    ClockNotFoundError,
    #[error("Variable \"{0}\" not found")]
    VariableNotFoundError(String)
}