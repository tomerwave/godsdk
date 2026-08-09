use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationRequest {
    pub source: PathBuf,
    pub output: PathBuf,
}

impl GenerationRequest {
    pub fn new(source: impl Into<PathBuf>, output: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            output: output.into(),
        }
    }

    pub fn source_path(&self) -> &Path {
        &self.source
    }

    pub fn output_path(&self) -> &Path {
        &self.output
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationResult;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GenerationError {
    #[error("SDK generation is not implemented yet")]
    NotImplemented,
}

pub fn generate(_request: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
    Err(GenerationError::NotImplemented)
}
