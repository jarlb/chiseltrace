use std::future::Future;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to validate input arguments: {0}")]
    ArgumentValidationError(String),
}

pub async fn map_err_to_string_async<F, T, E>(future: F) -> std::result::Result<T, String>
where
    F: Future<Output = Result<T, E>>,
    E: ToString
{
    future.await.map_err(|e| e.to_string())
}

pub fn map_err_to_string<F, T, E>(function: F) -> std::result::Result<T, String>
where
    F: FnOnce() -> Result<T, E>,
    E: ToString
{
    function().map_err(|e| e.to_string())
}