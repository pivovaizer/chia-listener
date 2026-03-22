# chia-listener

Event-driven Chia blockchain listener. Connects to multiple peers via DNS discovery, streams new blocks and transactions in real-time. Pure Rust, no full node required.

## Features

- **Real-time block streaming** — receive full blocks with parsed transactions, coin spends, additions and removals as they happen
- **Multi-peer connections** — connects to multiple peers simultaneously for reliability (configurable, default: 10)
- **Auto-reconnect** — automatically discovers and connects to new peers when connections drop
- **DNS discovery** — finds peers through Chia's DNS introducer network, no manual peer configuration needed
- **Auto-generated TLS** — generates and caches SSL certificates automatically, no Chia node installation required
- **Generator parser** — parses CLVM generator bytecode to extract coin spends from transaction blocks
- **Mainnet & Testnet** — supports both `mainnet` and `testnet11` networks

## Quick Start

```bash
git clone https://github.com/pivovaizer/chia-listener.git
cd chia-listener
cargo run --release
```

Output:

```
Listening for Chia blocks... (Ctrl+C to stop)

[+] Connected: 207.228.219.2:8444 (207.228.219.2:8444)
[+] Connected: 85.214.45.12:8444 (85.214.45.12:8444)
Peak: 8482267 (was None)

============================================================
Block #8482267
  Header: 0x3a7f...
  Additions: 12 | Removals: 8 | Spends: 5

  --- Transactions ---
  [1] Parent: 0xabc123...
      Puzzle: 0xdef456...
      Amount: 1750000000000 mojo (1.750000000000 XCH)
============================================================
```

## Usage as a Library

Add to your `Cargo.toml`:

```toml
[dependencies]
chia-block-listener = { git = "https://github.com/pivovaizer/chia-listener.git" }
tokio = { version = "1", features = ["full"] }
tokio-stream = { version = "0.1", features = ["sync"] }
```

### Basic Example

```rust
use chia_block_listener::{BlockListener, BlockListenerConfig};
use chia_block_listener::types::Event;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = BlockListenerConfig {
        auto_reconnect: true,
        network_id: "mainnet".into(),
        target_peers: 10,
        ..Default::default()
    };

    let listener = BlockListener::new(config)?;
    let rx = listener.subscribe();
    let mut events = BroadcastStream::new(rx);

    while let Some(Ok(event)) = events.next().await {
        match event {
            Event::BlockReceived(block) => {
                println!("Block #{} — {} spends", block.height, block.coin_spends.len());
            }
            Event::NewPeakHeight(e) => {
                println!("New peak: {}", e.new_peak);
            }
            _ => {}
        }
    }

    listener.shutdown_and_wait().await?;
    Ok(())
}
```

## Configuration

| Field | Type | Default | Description |
|---|---|---|---|
| `auto_reconnect` | `bool` | `false` | Automatically reconnect when peers disconnect |
| `network_id` | `String` | `"mainnet"` | Network to connect to (`mainnet`, `testnet11`) |
| `default_port` | `u16` | `8444` | Default peer port |
| `target_peers` | `usize` | `10` | Number of simultaneous peer connections to maintain |
| `max_auto_reconnect_retries` | `u32` | `10` | Max retry attempts per reconnect cycle |
| `buffer` | `usize` | `1024` | Broadcast channel buffer size per subscriber |

## Events

| Event | Description |
|---|---|
| `PeerConnected` | A new peer connection was established |
| `PeerDisconnected` | A peer connection was lost |
| `NewPeakHeight` | A peer reported a new blockchain peak height |
| `BlockReceived` | A full block was received and parsed (includes coin spends, additions, removals) |

## Block Data

Each `BlockReceived` event contains:

- `height` — block height
- `header_hash` — block header hash
- `timestamp` — block timestamp
- `coin_spends` — parsed transactions with puzzle reveal and solution
- `coin_additions` — new coins created
- `coin_removals` — coins spent
- `coin_creations` — coins created by spends
- `has_transactions_generator` — whether the block contains a CLVM generator
- `generator_size` — size of the generator bytecode

## Project Structure

```
chia-listener/
├── src/
│   ├── main.rs              # CLI demo binary
│   ├── lib.rs               # Library entry point
│   ├── block_listener.rs    # High-level listener with auto-reconnect
│   ├── peer_pool.rs         # Multi-peer connection pool with round-robin
│   ├── peer.rs              # Single peer connection & WebSocket handling
│   ├── protocol.rs          # Chia protocol message serialization
│   ├── dns_discovery.rs     # DNS peer discovery client
│   ├── tls.rs               # TLS certificate generation & loading
│   ├── types.rs             # Event types and configuration
│   └── error.rs             # Error types
├── crate/
│   ├── chia-generator-parser/   # CLVM generator bytecode parser
│   └── dig-dns-discovery/       # DNS resolver for Chia peers
└── Cargo.toml
```

## Requirements

- Rust 1.70+
- No Chia node installation required — certificates are auto-generated

## License

Apache-2.0
