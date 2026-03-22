use chia_listener::{init_tracing, BlockListener, BlockListenerConfig};
use chia_listener::types::Event;
use std::collections::HashSet;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

fn mojo_to_xch(mojo: &str) -> String {
    let val: u64 = mojo.parse().unwrap_or(0);
    format!("{:.12}", val as f64 / 1e12)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    // Auto-reconnect through DNS discovery
    let config = BlockListenerConfig {
        auto_reconnect: true,
        network_id: "mainnet".into(),
        default_port: 8444,
        max_auto_reconnect_retries: 10,
        target_peers: 10,
        buffer: 1024,
    };

    let listener = BlockListener::new(config)?;
    let rx = listener.subscribe();
    let mut events = BroadcastStream::new(rx);

    println!("Listening for Chia blocks... (Ctrl+C to stop)\n");

    let mut seen_blocks: HashSet<u32> = HashSet::new();

    while let Some(item) = events.next().await {
        match item {
            Ok(Event::PeerConnected(_)) | Ok(Event::PeerDisconnected(_)) => {}
            Ok(Event::NewPeakHeight(e)) => {
                println!("Peak: {} (was {:?})", e.new_peak, e.old_peak);
            }
            Ok(Event::BlockReceived(block)) => {
                if !seen_blocks.insert(block.height) {
                    continue; // already seen this block
                }

                if block.coin_spends.is_empty() {
                    println!("Block #{} - empty", block.height);
                    continue;
                }

                println!("\n{}", "=".repeat(60));
                println!("Block #{}", block.height);
                println!("  Header: {}", block.header_hash);
                println!("  Additions: {} | Removals: {} | Spends: {}",
                    block.coin_additions.len(),
                    block.coin_removals.len(),
                    block.coin_spends.len()
                );

                println!("\n  --- Transactions ---");
                for (i, spend) in block.coin_spends.iter().enumerate() {
                    let xch = mojo_to_xch(&spend.coin.amount);
                    println!("  [{}] Parent: {}", i + 1, spend.coin.parent_coin_info);
                    println!("      Puzzle: {}", spend.coin.puzzle_hash);
                    println!("      Amount: {} mojo ({} XCH)", spend.coin.amount, xch);
                }

                if !block.coin_additions.is_empty() {
                    println!("\n  --- New Coins (first 10) ---");
                    for (i, coin) in block.coin_additions.iter().take(10).enumerate() {
                        let xch = mojo_to_xch(&coin.amount);
                        println!("  [{}] Puzzle: {}", i + 1, coin.puzzle_hash);
                        println!("      Amount: {} mojo ({} XCH)", coin.amount, xch);
                    }
                    if block.coin_additions.len() > 10 {
                        println!("  ... and {} more", block.coin_additions.len() - 10);
                    }
                }
                println!("{}\n", "=".repeat(60));
            }
            Err(_) => {
                // Lagged - missed some events, continue
            }
        }
    }

    listener.shutdown_and_wait().await?;
    Ok(())
}
