use std::path::{Path, PathBuf};

/// The inputs accepted by the future SDK generation pipeline.
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

/// Marker for the artifact produced by a successful future generation run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationResult;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GenerationError {
    #[error("SDK generation is not implemented yet")]
    NotImplemented,
}

/// Run the SDK generation pipeline.
///
/// The pipeline is intentionally absent in the repository scaffold. In particular, this
/// function does not read the source, create the output directory, or access the network.
pub fn generate(_request: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
    Err(GenerationError::NotImplemented)
}
