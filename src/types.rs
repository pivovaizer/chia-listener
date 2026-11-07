// Rust-native event and data types used by the core crate
#[derive(Clone, Debug)]
pub struct PeerConnectedEvent {
    pub peer_id: String,
    pub host: String,
    pub port: u32,
}

#[derive(Clone, Debug)]
pub struct PeerDisconnectedEvent {
    pub peer_id: String,
    pub host: String,
    pub port: u32,
    pub message: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewPeakHeightEvent {
    pub old_peak: Option<u32>,
    pub new_peak: u32,
    pub peer_id: String,
}

// All hashes are lowercase hex strings without 0x prefix.
#[derive(Clone, Debug)]
pub struct CoinRecord {
    /// sha256(parent_coin_info || puzzle_hash || amount)
    pub coin_id: String,
    pub parent_coin_info: String,
    pub puzzle_hash: String,
    /// Amount in mojo (u64 as decimal string)
    pub amount: String,
    /// Farmer / pool reward coin
    pub coinbase: bool,
}

#[derive(Clone, Debug)]
pub struct CoinSpend {
    pub coin: CoinRecord,
    pub puzzle_reveal: String,
    pub solution: String,
    pub offset: u32,
}

#[derive(Clone, Debug)]
pub struct BlockReceivedEvent {
    pub peer_id: String,
    pub height: u32,
    /// u128 as decimal string
    pub weight: String,
    pub header_hash: String,
    pub prev_header_hash: String,
    /// u128 as decimal string
    pub total_iters: String,
    pub signage_point_index: u8,
    pub is_transaction_block: bool,
    /// Transaction blocks only
    pub prev_transaction_block_hash: Option<String>,
    /// Unix seconds; transaction blocks only
    pub timestamp: Option<u64>,
    pub farmer_puzzle_hash: String,
    pub pool_puzzle_hash: String,
    /// 48-byte G1 element, hex
    pub plot_public_key: String,
    /// Set for pooling (NFT) plots
    pub pool_contract_puzzle_hash: Option<String>,
    /// User fees in mojo; transaction blocks only
    pub fees: Option<u64>,
    /// CLVM cost; transaction blocks only
    pub cost: Option<u64>,
    pub generator_ref_list: Vec<u32>,
    pub coin_additions: Vec<CoinRecord>,
    pub coin_removals: Vec<CoinRecord>,
    pub coin_spends: Vec<CoinSpend>,
    pub coin_creations: Vec<CoinRecord>,
    pub has_transactions_generator: bool,
    pub generator_size: u32,
}

// Unified event enum for Rust consumers (Listener facade)
#[derive(Clone, Debug)]
pub enum Event {
    PeerConnected(PeerConnectedEvent),
    PeerDisconnected(PeerDisconnectedEvent),
    NewPeakHeight(NewPeakHeightEvent),
    BlockReceived(BlockReceivedEvent),
}

// Configuration for Listener / event buffering
#[derive(Clone, Debug)]
pub struct BlockListenerConfig {
    pub buffer: usize, // ring buffer size per subscriber
    pub auto_reconnect: bool,
    pub network_id: String,
    pub default_port: u16,
    pub max_auto_reconnect_retries: u32,
    pub target_peers: usize, // desired number of simultaneous peer connections
}

impl Default for BlockListenerConfig {
    fn default() -> Self {
        Self {
            buffer: 1024,
            auto_reconnect: false,
            network_id: "mainnet".to_string(),
            default_port: 8444,
            max_auto_reconnect_retries: 10,
            target_peers: 10,
        }
    }
}
