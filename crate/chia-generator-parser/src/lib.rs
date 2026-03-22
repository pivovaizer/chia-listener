pub mod error;
pub mod parser;
pub mod types;

pub use error::*;
use hex::FromHexError;
pub use parser::*;
pub use types::*;

pub fn string_to_bytes32(string: &String) -> Result<Bytes32> {
    let u8_vec = hex::decode(string)?;
    Bytes32::try_from(u8_vec)
        .map_err(|_| GeneratorParserError::HexDecodingError(FromHexError::InvalidStringLength))
}
