use dig_dns_discovery::{DnsDiscovery, DnsDiscoveryError};
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), DnsDiscoveryError> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting DNS Discovery Example");

    // Create the DNS discovery instance
    let discovery = DnsDiscovery::new().await?;

    // Discover mainnet peers
    info!("Discovering Chia mainnet peers...");
    match discovery.discover_mainnet_peers().await {
        Ok(result) => {
            info!("Mainnet discovery successful!");
            print_discovery_result("Mainnet", &result);
        }
        Err(e) => {
            eprintln!("❌ Mainnet discovery failed: {}", e);
        }
    }

    println!("\n{}", "=".repeat(80));

    // Discover testnet11 peers
    info!("Discovering Chia testnet11 peers...");
    match discovery.discover_testnet11_peers().await {
        Ok(result) => {
            info!("Testnet11 discovery successful!");
            print_discovery_result("Testnet11", &result);
        }
        Err(e) => {
            eprintln!("❌ Testnet11 discovery failed: {}", e);
        }
    }

    println!("\n{}", "=".repeat(80));

    // Test individual hostname resolution
    info!("Testing individual hostname resolution...");

    let test_hostname = "dns-introducer.chia.net";
    info!("Testing resolution for: {}", test_hostname);

    match discovery.resolve_both(test_hostname, 8444).await {
        Ok(result) => {
            print_discovery_result(&format!("Individual lookup for {}", test_hostname), &result);
        }
        Err(e) => {
            eprintln!("❌ Individual lookup failed: {}", e);
        }
    }

    Ok(())
}

fn print_discovery_result(network: &str, result: &dig_dns_discovery::DiscoveryResult) {
    println!("\n📋 {} Discovery Results:", network);
    println!("   Total peers found: {}", result.total_count);
    println!("   IPv4 peers: {}", result.ipv4_peers.len());
    println!("   IPv6 peers: {}", result.ipv6_peers.len());

    if !result.ipv4_peers.is_empty() {
        println!("\n🌍 IPv4 Peers:");
        for (i, peer) in result.ipv4_peers.iter().enumerate() {
            println!("   {}. {}", i + 1, peer.display_address());
        }
    }

    if !result.ipv6_peers.is_empty() {
        println!("\n🌐 IPv6 Peers:");
        for (i, peer) in result.ipv6_peers.iter().enumerate() {
            println!("   {}. {}", i + 1, peer.display_address());
        }
    }

    if result.total_count == 0 {
        println!("   ⚠️  No peers found");
    }
}
