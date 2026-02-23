pub mod generator;
pub mod konata;
pub mod model;

use model::PipelineTrace;
use std::io::Read;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TraceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error at line {line}: {message}")]
    Parse { line: usize, message: String },
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
}

/// Abstract trace data source.
pub trait TraceSource {
    #[allow(dead_code)]
    fn format_name(&self) -> &str;
    fn file_extensions(&self) -> &[&str];
    fn load(&self, reader: &mut dyn Read) -> Result<PipelineTrace, TraceError>;
    fn detect(&self, first_bytes: &[u8]) -> bool {
        let _ = first_bytes;
        false
    }
}

/// Registry of trace format parsers.
pub struct TraceRegistry {
    sources: Vec<Box<dyn TraceSource>>,
}

impl TraceRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            sources: Vec::new(),
        };
        reg.register(Box::new(konata::KonataSource));
        reg
    }

    pub fn register(&mut self, source: Box<dyn TraceSource>) {
        self.sources.push(source);
    }

    /// Load a trace file, auto-detecting format by extension or content.
    pub fn load_file(&self, path: &std::path::Path) -> Result<PipelineTrace, TraceError> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // Try by extension first.
        for source in &self.sources {
            if source.file_extensions().contains(&ext) {
                let mut file = std::fs::File::open(path)?;
                return source.load(&mut file);
            }
        }

        // Try by content detection.
        let mut file = std::fs::File::open(path)?;
        let mut buf = [0u8; 1024];
        let n = file.read(&mut buf)?;
        drop(file);

        for source in &self.sources {
            if source.detect(&buf[..n]) {
                let mut file = std::fs::File::open(path)?;
                return source.load(&mut file);
            }
        }

        Err(TraceError::UnsupportedFormat(format!(
            "No parser found for: {}",
            path.display()
        )))
    }
}

impl Default for TraceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
