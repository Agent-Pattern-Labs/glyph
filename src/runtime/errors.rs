use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{message}")]
pub struct GlyphRuntimeError {
    pub message: String,
    pub step_id: Option<String>,
}

impl GlyphRuntimeError {
    pub fn new(message: impl Into<String>, step_id: Option<String>) -> Self {
        let message = message.into();
        let message = match &step_id {
            Some(step_id) => format!("{message} at {step_id}"),
            None => message,
        };

        Self { message, step_id }
    }
}
