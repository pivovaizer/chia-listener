use chia_protocol::{Bytes32, ProtocolMessageTypes};
use chia_traits::Streamable;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[allow(dead_code)]
pub const MAINNET_GENESIS_CHALLENGE: &str =
    "ccd5bb71183532bff220ba46c268991a3ff07eb358e8255a65c30a2dce0e5fbb";
#[allow(dead_code)]
pub const TESTNET11_GENESIS_CHALLENGE: &str =
    "37a90eb5185a9c4439a91ddc98bbadce7b4feba060d50116a067de66bf236615";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handshake {
    pub network_id: String,
    pub protocol_version: String,
    pub software_version: String,
    pub server_port: u16,
    pub node_type: u8,
    pub capabilities: Vec<(u16, String)>,
}

impl Handshake {
    #[allow(dead_code)]
    pub fn new(network_id: String, port: u16) -> Self {
        Self {
            network_id,
            protocol_version: "0.0.38".to_string(),
            software_version: "1.0.0".to_string(),
            server_port: port,
            node_type: 1, // FULL_NODE
            capabilities: vec![(1, "base".to_string()), (2, "full_node".to_string())],
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Message {
    pub msg_type: ProtocolMessageTypes,
    pub id: Option<u16>,
    pub data: Vec<u8>,
}

#[allow(dead_code)]
impl Message {
    pub fn new(msg_type: ProtocolMessageTypes, id: Option<u16>, data: Vec<u8>) -> Self {
        Self { msg_type, id, data }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, std::io::Error> {
        // Message format:
        // - msg_type (1 byte)
        // - optional id (2 bytes if present)
        // - data_len (4 bytes, big-endian)
        // - data

        let mut bytes = Vec::new();
        bytes.push(self.msg_type as u8);

        if let Some(id) = self.id {
            bytes.push(1); // has_id flag
            bytes.extend_from_slice(&id.to_be_bytes());
        } else {
            bytes.push(0); // no id
        }

        let data_len = self.data.len() as u32;
        bytes.extend_from_slice(&data_len.to_be_bytes());
        bytes.extend_from_slice(&self.data);

        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, std::io::Error> {
        use std::io::{Error, ErrorKind};

        if bytes.len() < 6 {
            return Err(Error::new(ErrorKind::InvalidData, "Message too short"));
        }

        let mut msg_type_bytes_parse_cursor = std::io::Cursor::new(&bytes[0..1]);
        let msg_type = ProtocolMessageTypes::parse::<false>(&mut msg_type_bytes_parse_cursor)
            .map_err(|_| Error::new(ErrorKind::InvalidData, "Failed to parse message type"))?;
        let has_id = bytes[1] != 0;

        let (id, data_start) = if has_id {
            if bytes.len() < 8 {
                return Err(Error::new(ErrorKind::InvalidData, "Message too short"));
            }
            let id = u16::from_be_bytes([bytes[2], bytes[3]]);
            (Some(id), 8)
        } else {
            (None, 6)
        };

        let data_len = u32::from_be_bytes([
            bytes[data_start - 4],
            bytes[data_start - 3],
            bytes[data_start - 2],
            bytes[data_start - 1],
        ]);

        let data = bytes[data_start..].to_vec();

        if data.len() != data_len as usize {
            return Err(Error::new(ErrorKind::InvalidData, "Data length mismatch"));
        }

        Ok(Self { msg_type, id, data })
    }
}

#[allow(dead_code)]
pub fn calculate_node_id(cert_der: &[u8]) -> Result<Bytes32, std::io::Error> {
    let mut hasher = Sha256::new();
    hasher.update(cert_der);
    let result = hasher.finalize();

    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&result);
    Ok(Bytes32::new(bytes))
}
