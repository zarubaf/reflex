pub mod generator;
pub mod model;
pub mod uscope_source;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum TraceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
}
