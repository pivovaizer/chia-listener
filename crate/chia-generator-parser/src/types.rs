use serde::{Deserialize, Serialize};

pub use chia_protocol::{Bytes32, Coin};

// Basic numeric types - use standard Rust types for simplicity
pub type Uint32 = u32;
pub type Uint64 = u64;
pub type Uint128 = u128;

// SerializedProgram placeholder - use Vec<u8> for now
pub type SerializedProgram = Vec<u8>;

/// Block height reference  
pub type BlockHeight = Uint32;

/// Comprehensive parsed block information including all coin data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedBlock {
    /// Block height
    pub height: u32,

    /// Block weight
    pub weight: String,

    /// Block header hash (hex string for serialization)
    pub header_hash: String,

    /// Header hash of the previous block (hex). Used for reorg detection.
    pub prev_header_hash: String,

    /// Total VDF iterations up to this block
    pub total_iters: String,

    pub signage_point_index: u8,

    /// Transaction blocks carry timestamp, fees, cost and a generator
    pub is_transaction_block: bool,

    /// Header hash of the previous transaction block (hex); transaction blocks only
    pub prev_transaction_block_hash: Option<String>,

    /// Block timestamp in unix seconds; transaction blocks only
    pub timestamp: Option<u64>,

    /// Farmer reward target (hex)
    pub farmer_puzzle_hash: String,

    /// Pool reward target (hex)
    pub pool_puzzle_hash: String,

    /// Plot public key from the proof of space (hex, 48 bytes)
    pub plot_public_key: String,

    /// Pool contract puzzle hash (hex); set for pooling (NFT) plots
    pub pool_contract_puzzle_hash: Option<String>,

    /// User fees in mojo; transaction blocks only
    pub fees: Option<u64>,

    /// CLVM cost; transaction blocks only
    pub cost: Option<u64>,

    /// Heights of previous blocks whose generators this block references
    pub generator_ref_list: Vec<u32>,

    /// Coin additions (new coins created)
    pub coin_additions: Vec<CoinInfo>,

    /// Coin removals (coins spent)
    pub coin_removals: Vec<CoinInfo>,

    /// Detailed coin spends (if generator present)
    pub coin_spends: Vec<CoinSpendInfo>,

    /// Coins created by spends (if generator present)
    pub coin_creations: Vec<CoinInfo>,

    /// Whether block has transactions generator
    pub has_transactions_generator: bool,

    /// Generator size in bytes
    pub generator_size: Option<u32>,
}

/// Information about a coin (unspent transaction output)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinInfo {
    /// Coin ID (hex string): sha256(parent_coin_info || puzzle_hash || amount)
    pub coin_id: String,

    /// Parent coin ID (hex string)
    pub parent_coin_info: String,

    /// Puzzle hash (hex string)
    pub puzzle_hash: String,

    /// Amount in mojos
    pub amount: Uint64,

    /// True for farmer / pool reward coins (reward claims incorporated in the block)
    #[serde(default)]
    pub coinbase: bool,
}

impl CoinInfo {
    pub fn new(parent_coin_info: Bytes32, puzzle_hash: Bytes32, amount: Uint64) -> Self {
        let coin_id = Coin::new(parent_coin_info, puzzle_hash, amount).coin_id();
        Self {
            coin_id: hex::encode(coin_id),
            parent_coin_info: hex::encode(parent_coin_info),
            puzzle_hash: hex::encode(puzzle_hash),
            amount,
            coinbase: false,
        }
    }

    /// Create from raw bytes. Returns None unless both hashes are 32 bytes.
    pub fn from_bytes(parent_coin_info: &[u8], puzzle_hash: &[u8], amount: Uint64) -> Option<Self> {
        let parent = Bytes32::try_from(parent_coin_info).ok()?;
        let puzzle = Bytes32::try_from(puzzle_hash).ok()?;
        Some(Self::new(parent, puzzle, amount))
    }

    /// Mark as a farmer / pool reward coin
    pub fn into_coinbase(mut self) -> Self {
        self.coinbase = true;
        self
    }
}

/// Detailed coin spend information with transaction data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinSpendInfo {
    pub coin: CoinInfo,
    pub puzzle_reveal: String,
    pub solution: String,
    pub real_data: bool,
    pub parsing_method: String,
    pub offset: Uint32,
    pub created_coins: Vec<CoinInfo>,
}

impl CoinSpendInfo {
    pub fn new(
        coin: CoinInfo,
        puzzle_reveal: String,
        solution: String,
        real_data: bool,
        parsing_method: String,
        offset: Uint32,
        created_coins: Vec<CoinInfo>,
    ) -> Self {
        Self {
            coin,
            puzzle_reveal,
            solution,
            real_data,
            parsing_method,
            offset,
            created_coins,
        }
    }
}

/// Information about a block's transactions generator
#[derive(Debug, Clone)]
pub struct GeneratorBlockInfo {
    /// Previous block header hash
    pub prev_header_hash: Bytes32,

    /// The transactions generator program (CLVM bytecode)
    pub transactions_generator: Option<SerializedProgram>,

    /// List of block heights that this generator references
    pub transactions_generator_ref_list: Vec<BlockHeight>,
}

impl GeneratorBlockInfo {
    pub fn new(
        prev_header_hash: Bytes32,
        transactions_generator: Option<SerializedProgram>,
        transactions_generator_ref_list: Vec<BlockHeight>,
    ) -> Self {
        Self {
            prev_header_hash,
            transactions_generator,
            transactions_generator_ref_list,
        }
    }
}

/// Information about a block's height and type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeightInfo {
    /// Block height
    pub height: Uint32,

    /// Whether this is a transaction block
    pub is_transaction_block: bool,
}

/// Analysis information about a generator's bytecode
#[derive(Debug, Clone)]
pub struct GeneratorAnalysis {
    /// Total size in bytes
    pub size_bytes: usize,
    /// Whether the bytecode is empty
    pub is_empty: bool,
    /// Heuristic: presence of common CLVM opcode patterns
    pub contains_clvm_patterns: bool,
    /// Heuristic: presence of 32-byte sequences typical for coins
    pub contains_coin_patterns: bool,
    /// Shannon entropy estimate
    pub entropy: f64,
}

/// Result of parsing a generator, including basic analysis and block context
#[derive(Debug, Clone)]
pub struct ParsedGenerator {
    pub block_info: GeneratorBlockInfo,
    pub generator_hex: Option<String>,
    pub analysis: GeneratorAnalysis,
}

impl BlockHeightInfo {
    pub fn new(height: Uint32, is_transaction_block: bool) -> Self {
        Self {
            height,
            is_transaction_block,
        }
    }
}
