use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChiaError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Handshake error: {0}")]
    Handshake(String),

    #[error("WebSocket error: {0}")]
    WebSocket(#[from] Box<tokio_tungstenite::tungstenite::Error>),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Other error: {0}")]
    Other(String),
}

// Helper conversion for unboxed WebSocket errors
impl From<tokio_tungstenite::tungstenite::Error> for ChiaError {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        ChiaError::WebSocket(Box::new(err))
    }
}
