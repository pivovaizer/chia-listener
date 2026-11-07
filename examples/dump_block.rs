//! Downloads full blocks by height and saves their raw bytes as test fixtures.
//!
//! Usage:
//!   cargo run --example dump_block -- 9540781 9540780
//!
//! Writes tests/fixtures/block_<height>.bin (serialized `FullBlock`).

use chia_listener::dns_discovery::DnsDiscoveryClient;
use chia_listener::peer::PeerConnection;
use chia_traits::Streamable;
use std::path::Path;

const MAX_PEERS_TO_TRY: usize = 10;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let heights: Vec<u64> = std::env::args()
        .skip(1)
        .map(|a| a.parse())
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow::anyhow!("heights must be numbers: {e}"))?;
    if heights.is_empty() {
        anyhow::bail!("usage: cargo run --example dump_block -- <height> [height ...]");
    }

    let out_dir = Path::new("tests/fixtures");
    std::fs::create_dir_all(out_dir)?;

    let discovery = DnsDiscoveryClient::new().await?;
    let peers = discovery.discover_mainnet_peers().await?;
    println!("Discovered {} IPv4 peers", peers.ipv4_peers.len());

    for peer in peers.ipv4_peers.iter().take(MAX_PEERS_TO_TRY) {
        let host = peer.host.to_string();
        println!("Trying {}:{} ...", host, peer.port);

        let conn = PeerConnection::new(host.clone(), peer.port, "mainnet".into());
        let mut ws = match conn.connect().await {
            Ok(ws) => ws,
            Err(e) => {
                println!("  connect failed: {e}");
                continue;
            }
        };
        if let Err(e) = conn.handshake(&mut ws).await {
            println!("  handshake failed: {e}");
            continue;
        }

        let mut all_ok = true;
        for &height in &heights {
            match conn.request_block_by_height(height, &mut ws).await {
                Ok(block) => {
                    let bytes = block.to_bytes()?;
                    let path = out_dir.join(format!("block_{height}.bin"));
                    std::fs::write(&path, &bytes)?;
                    println!("  saved {} ({} bytes)", path.display(), bytes.len());
                }
                Err(e) => {
                    println!("  block {height} failed: {e}");
                    all_ok = false;
                    break;
                }
            }
        }
        if all_ok {
            return Ok(());
        }
    }

    anyhow::bail!("could not download blocks from any of {MAX_PEERS_TO_TRY} peers")
}
