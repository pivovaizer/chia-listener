# chia-listener

Event-driven Chia blockchain listener and indexer. Connects to multiple peers via DNS discovery, streams new blocks and transactions in real time and stores them in PostgreSQL. Pure Rust, no full node installation required.

> **Status:** work in progress. Block streaming works; writing blocks to the database is being implemented. The goal is a backend for a Chia blockchain explorer.

## Features

- **Real-time block streaming** — receive full blocks with parsed transactions, coin spends, additions and removals as they happen
- **Multi-peer connections** — connects to multiple peers simultaneously for reliability (configurable, default: 10)
- **Auto-reconnect** — automatically discovers and connects to new peers when connections drop
- **DNS discovery** — finds peers through Chia's DNS introducer network, no manual peer configuration needed
- **Auto-generated TLS** — generates and caches SSL certificates signed by the public Chia CA, no Chia node installation required
- **Generator parser** — parses CLVM generator bytecode to extract coin spends from transaction blocks
- **PostgreSQL storage** — schema and Docker setup included
- **Mainnet & Testnet** — supports both `mainnet` and `testnet11` networks

## How it connects

The listener performs the Chia handshake as a **full node** (`NodeType::FullNode`). Peers announce new peaks with `NewPeak`, and the listener answers with `RequestBlock` to get the full block. Connecting as a wallet does not work for this: full nodes close the connection (`CloseFrame { code: Protocol }`) when a wallet sends `RequestBlock`.

## Quick Start

Requirements: Rust (stable), Docker with Compose.

```bash
git clone https://github.com/pivovaizer/chia-listener.git
cd chia-listener

# 1. Configure: copy the example and set a password
cp .env-example .env
#    edit POSTGRES_PASSWORD and the password in DATABASE_URL (they must match)

# 2. Start PostgreSQL (container "rustlistener", port 127.0.0.1:5433)
docker compose up -d

# 3. Run the listener (applies DB migrations on startup)
cargo run --release
```

Output:

```
=== Latest block height: None ===
Listening for Chia blocks... (Ctrl+C to stop)

Peak: 9540409 (was Some(9540408))

============================================================
Block #9540409
  Header: 4cf5d242...
  Additions: 4126 | Removals: 17 | Spends: 17

  --- Transactions ---
  [1] Parent: b6c4258b...
      Puzzle: 5f5f7d5e...
      Amount: 98801820 mojo (0.000098801820 XCH)
============================================================
```

Non-transaction blocks are printed as `Block #N - empty`. They contain only the farmer and pool reward coins.

Verbose peer logging:

```bash
RUST_LOG=chia_listener::peer=debug cargo run
```

### Database

| | |
|---|---|
| Container | `rustlistener` (`postgres:16`) |
| Address | `127.0.0.1:5433` |
| Schema | [`migrations/`](migrations/), applied automatically on startup ([refinery](https://github.com/rust-db/refinery)) |
| Data | Docker volume `rustlistener_data` |

To change the schema, add a new file `migrations/V<N>__<description>.sql`, e.g. `V2__hints.sql`. Never edit a migration that has already been applied. Applied versions are tracked in the `refinery_schema_history` table.

```bash
docker exec -it rustlistener psql -U listener -d chia   # open psql
docker compose down -v && docker compose up -d          # recreate the DB (deletes all data!)
```

## Tests

```bash
cargo test --workspace                 # offline tests
cargo test --workspace -- --ignored    # tests that need network access (DNS)
```

The parser rejects blocks it cannot parse reliably (invalid generator, spend count mismatch, references to previous generators) with an error instead of reporting them as empty. Tests cover this behaviour and the `coin_id` computation.

## Usage as a Library

Add to your `Cargo.toml`:

```toml
[dependencies]
chia-listener = { git = "https://github.com/pivovaizer/chia-listener.git" }
tokio = { version = "1", features = ["full"] }
tokio-stream = { version = "0.1", features = ["sync"] }
```

### Basic Example

```rust
use chia_listener::{BlockListener, BlockListenerConfig};
use chia_listener::types::Event;
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

Environment (`.env`):

| Variable | Description |
|---|---|
| `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB` | Used by Docker Compose to create the database |
| `DATABASE_URL` | Connection string used by the listener |

## Events

| Event | Description |
|---|---|
| `PeerConnected` | A new peer connection was established |
| `PeerDisconnected` | A peer connection was lost |
| `NewPeakHeight` | A peer reported a new blockchain peak height |
| `BlockReceived` | A full block was received and parsed (includes coin spends, additions, removals) |

The same block usually arrives from several peers. Deduplicate by `height` / `header_hash`.

## Block Data

Each `BlockReceived` event contains:

All hashes are lowercase hex strings.

- `height`, `header_hash`, `prev_header_hash` — block identity and chain link
- `weight`, `total_iters` — cumulative chain weight and VDF iterations (u128 as string)
- `signage_point_index`
- `is_transaction_block` — only transaction blocks have a timestamp, fees, cost and a generator
- `prev_transaction_block_hash` — previous transaction block (transaction blocks only)
- `timestamp` — unix seconds (`None` for non-transaction blocks)
- `farmer_puzzle_hash`, `pool_puzzle_hash` — reward targets
- `plot_public_key`, `pool_contract_puzzle_hash` — proof of space (pool contract only for pooling plots)
- `fees`, `cost` — user fees in mojo and CLVM cost (transaction blocks only)
- `generator_ref_list` — heights of previous generators referenced by this block
- `peer_id` — peer the block came from
- `coin_spends` — parsed transactions with puzzle reveal, solution and position in the generator
- `coin_additions` — new coins (reward coins + coins created by spends)
- `coin_removals` — coins spent
- `coin_creations` — coins created by spends
- `has_transactions_generator`, `generator_size` — CLVM generator presence and size

Every coin (`CoinRecord`) has a `coin_id` (`sha256(parent_coin_info || puzzle_hash || amount)`) and a `coinbase` flag for farmer / pool rewards.

## Project Structure

```
chia-listener/
├── src/
│   ├── main.rs              # CLI binary: listens and prints blocks
│   ├── lib.rs               # Library entry point
│   ├── block_listener.rs    # High-level listener with auto-reconnect
│   ├── peer_pool.rs         # Multi-peer connection pool with failover
│   ├── peer.rs              # Single peer connection, handshake & WebSocket handling
│   ├── protocol.rs          # Chia protocol message serialization
│   ├── dns_discovery.rs     # DNS peer discovery client
│   ├── tls.rs               # TLS certificate generation & loading
│   ├── database.rs          # PostgreSQL connection and queries
│   ├── types.rs             # Event types and configuration
│   └── error.rs             # Error types
├── crate/
│   ├── chia-generator-parser/   # CLVM generator bytecode parser
│   └── dig-dns-discovery/       # DNS resolver for Chia peers
├── migrations/              # SQL schema migrations (applied on startup)
├── build.rs                 # Rebuilds when migrations change
├── docker-compose.yml       # PostgreSQL container
├── .env-example             # Environment template
└── Cargo.toml
```

## Roadmap

- [x] Real-time block streaming from multiple peers
- [x] PostgreSQL setup
- [ ] Store blocks, coins (with `coin_id` and spent status) and spends
- [ ] Reorg handling, gap detection and historical backfill
- [ ] Spend conditions, hints, CAT / NFT / DID detection
- [ ] HTTP API for the explorer frontend

## License

Apache-2.0
