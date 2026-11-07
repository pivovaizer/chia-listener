// Rust-native event and data types used by the core crate
use chia_generator_parser::{CoinInfo, ParsedBlock};

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

impl CoinRecord {
    pub fn from_parsed(coin: &CoinInfo) -> Self {
        CoinRecord {
            coin_id: coin.coin_id.clone(),
            parent_coin_info: coin.parent_coin_info.clone(),
            puzzle_hash: coin.puzzle_hash.clone(),
            amount: coin.amount.to_string(),
            coinbase: coin.coinbase,
        }
    }
}

impl BlockReceivedEvent {
    /// Builds the public event from the parser output.
    pub fn from_parsed(parsed: &ParsedBlock, peer_id: String) -> Self {
        let to_records =
            |coins: &[CoinInfo]| coins.iter().map(CoinRecord::from_parsed).collect::<Vec<_>>();

        BlockReceivedEvent {
            peer_id,
            height: parsed.height,
            weight: parsed.weight.clone(),
            header_hash: parsed.header_hash.clone(),
            prev_header_hash: parsed.prev_header_hash.clone(),
            total_iters: parsed.total_iters.clone(),
            signage_point_index: parsed.signage_point_index,
            is_transaction_block: parsed.is_transaction_block,
            prev_transaction_block_hash: parsed.prev_transaction_block_hash.clone(),
            timestamp: parsed.timestamp,
            farmer_puzzle_hash: parsed.farmer_puzzle_hash.clone(),
            pool_puzzle_hash: parsed.pool_puzzle_hash.clone(),
            plot_public_key: parsed.plot_public_key.clone(),
            pool_contract_puzzle_hash: parsed.pool_contract_puzzle_hash.clone(),
            fees: parsed.fees,
            cost: parsed.cost,
            generator_ref_list: parsed.generator_ref_list.clone(),
            coin_additions: to_records(&parsed.coin_additions),
            coin_removals: to_records(&parsed.coin_removals),
            coin_spends: parsed
                .coin_spends
                .iter()
                .map(|spend| CoinSpend {
                    coin: CoinRecord::from_parsed(&spend.coin),
                    puzzle_reveal: spend.puzzle_reveal.clone(),
                    solution: spend.solution.clone(),
                    offset: spend.offset,
                })
                .collect(),
            coin_creations: to_records(&parsed.coin_creations),
            has_transactions_generator: parsed.has_transactions_generator,
            generator_size: parsed.generator_size.unwrap_or(0),
        }
    }
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
