use thiserror::Error;

#[derive(Error, Debug)]
pub enum GeneratorParserError {
    #[error("Invalid block format: {0}")]
    InvalidBlockFormat(String),

    #[error("Buffer too short: expected at least {expected} bytes, got {actual}")]
    BufferTooShort { expected: usize, actual: usize },

    #[error("Invalid generator bytecode: {0}")]
    InvalidGeneratorBytecode(String),

    #[error("CLVM parsing error: {0}")]
    ClvmParsingError(String),

    #[error("CLVM execution error: {0}")]
    ClvmExecutionError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Hex decoding error: {0}")]
    HexDecodingError(#[from] hex::FromHexError),
}

pub type Result<T> = std::result::Result<T, GeneratorParserError>;
