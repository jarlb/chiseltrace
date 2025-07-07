use thiserror::Error;
use tywaves_rs::hgldd;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid slicing statement: {0}")]
    StatementLookupError(String),
    #[error("Clock signal not found")]
    ClockNotFoundError,
    #[error("Variable \"{0}\" not found")]
    VariableNotFoundError(String),
    #[error("HGLDD reader error: {0:?}")]
    HGLDDError(hgldd::reader::HglddReaderError),
    #[error("TyVCD builder error: {0:?}")]
    TyVCDBuilderError(tywaves_rs::tyvcd::builder::BuilderError),
    #[error("TyVCD rewriter error: {0:?}")]
    TyVCDRewriterError(tywaves_rs::vcd_rewrite::VcdRewriteError),
    #[error("Tywaves signal not found")]
    TywavesSignalNotFound,
    #[error("Tywaves variable downcast failed")]
    TywavesDowncastFailed
}

// Auto-implementation did not work
impl From<hgldd::reader::HglddReaderError> for Error {
    fn from(value: hgldd::reader::HglddReaderError) -> Self {
        Error::HGLDDError(value)
    }
}

impl From<tywaves_rs::tyvcd::builder::BuilderError> for Error {
    fn from(value: tywaves_rs::tyvcd::builder::BuilderError) -> Self {
        Error::TyVCDBuilderError(value)
    }
}

impl From<tywaves_rs::vcd_rewrite::VcdRewriteError> for Error {
    fn from(value: tywaves_rs::vcd_rewrite::VcdRewriteError) -> Self {
        Error::TyVCDRewriterError(value)
    }
}