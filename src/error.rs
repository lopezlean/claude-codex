use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),
}

impl From<clap::Error> for AppError {
    fn from(value: clap::Error) -> Self {
        Self::Message(value.to_string())
    }
}
