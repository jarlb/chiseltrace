use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid slicing statement: {0}")]
    StatementLookupError(String)
}