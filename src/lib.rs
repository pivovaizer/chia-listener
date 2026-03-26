#![deny(clippy::all)]

pub mod block_listener;
pub mod dns_discovery;
pub mod error;
pub mod peer;
pub mod peer_pool;
pub mod protocol;
pub mod tls;
pub mod types;
pub mod database;

pub use block_listener::BlockListener;
pub use chia_generator_parser::{string_to_bytes32, GeneratorParserError};
pub use dns_discovery::{DiscoveryResult, DnsDiscoveryClient, DnsDiscoveryError, PeerAddress};
pub use types::BlockListenerConfig;

pub fn init_tracing() {
    // Initialize logging with a filter for our crate
    use tracing_subscriber::{filter::LevelFilter, fmt, EnvFilter};

    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy()
        .add_directive("chia_listener=info".parse().unwrap());

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(false)
        .init();
}
