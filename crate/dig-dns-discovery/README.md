# DNS Discovery Crate

An isolated Rust crate for discovering Chia network peers using DNS introducers. This crate properly handles both IPv4 (A records) and IPv6 (AAAA records) lookups, returning separate lists for each protocol version.

## Features

- 🌍 **IPv4 Support**: Performs A record DNS lookups for IPv4 addresses
- 🌐 **IPv6 Support**: Performs AAAA record DNS lookups for IPv6 addresses  
- 🔀 **Separate Results**: Returns IPv4 and IPv6 peers in separate lists
- 🎲 **Randomization**: Shuffles peer lists for load distribution
- 🏷️ **Proper Formatting**: IPv6 addresses are properly formatted with brackets for URL usage
- 🕸️ **Multiple Networks**: Built-in support for Chia mainnet and testnet11
- 🛠️ **Customizable**: Generic discovery for any list of DNS introducers

## Usage

### Basic Usage

```rust
use dns_discovery::{DnsDiscovery, DnsDiscoveryError};

#[tokio::main]
async fn main() -> Result<(), DnsDiscoveryError> {
    // Create DNS discovery instance
    let discovery = DnsDiscovery::new().await?;
    
    // Discover mainnet peers
    let result = discovery.discover_mainnet_peers().await?;
    
    println!("Found {} IPv4 peers and {} IPv6 peers", 
             result.ipv4_peers.len(), 
             result.ipv6_peers.len());
    
    // Access individual peer lists
    for peer in &result.ipv4_peers {
        println!("IPv4 Peer: {}", peer.display_address());
    }
    
    for peer in &result.ipv6_peers {
        println!("IPv6 Peer: {}", peer.display_address());
    }
    
    Ok(())
}
```

### Network-Specific Discovery

```rust
// Mainnet discovery
let mainnet_peers = discovery.discover_mainnet_peers().await?;

// Testnet11 discovery  
let testnet_peers = discovery.discover_testnet11_peers().await?;

// Custom introducers
let custom_peers = discovery.discover_peers(
    &["custom-introducer.example.com"], 
    8444
).await?;
```

### Individual Protocol Lookups

```rust
// IPv4 only (A records)
let ipv4_addrs = discovery.resolve_ipv4("dns-introducer.chia.net").await?;

// IPv6 only (AAAA records)
let ipv6_addrs = discovery.resolve_ipv6("dns-introducer.chia.net").await?;

// Both protocols
let both = discovery.resolve_both("dns-introducer.chia.net", 8444).await?;
```

## Data Structures

### `DiscoveryResult`

Contains the results of DNS discovery:

```rust
pub struct DiscoveryResult {
    pub ipv4_peers: Vec<PeerAddress>,  // IPv4 peer addresses
    pub ipv6_peers: Vec<PeerAddress>,  // IPv6 peer addresses  
    pub total_count: usize,            // Total number of peers
}
```

### `PeerAddress`

Represents a single peer address:

```rust
pub struct PeerAddress {
    pub host: IpAddr,       // IP address (IPv4 or IPv6)
    pub port: u16,          // Port number
    pub is_ipv6: bool,      // True if IPv6, false if IPv4
}
```

The `display_address()` method properly formats addresses:
- IPv4: `192.168.1.1:8444`
- IPv6: `[2001:db8::1]:8444` (with brackets)

## Built-in DNS Introducers

### Mainnet (port 8444)
- `dns-introducer.chia.net`
- `chia.ctrlaltdel.ch`
- `seeder.dexie.space`
- `chia.hoffmang.com`

### Testnet11 (port 58444)
- `dns-introducer-testnet11.chia.net`

## Key Differences from JavaScript Implementation

This Rust implementation provides several improvements over the JavaScript version in `coin-monitor.js`:

1. **Explicit Protocol Separation**: Uses separate A and AAAA lookups instead of generic resolution
2. **Type Safety**: Strongly typed IP addresses and error handling
3. **Proper IPv6 Formatting**: Automatically handles bracket formatting for IPv6 URLs
4. **Performance**: Concurrent DNS lookups with proper async/await
5. **Error Handling**: Detailed error types with proper propagation

## Running the Example

```bash
cd crate/dig-dns-discovery
cargo run --example discover_peers
```

This will discover peers for both mainnet and testnet11, showing separate IPv4 and IPv6 results.

## Testing

```bash
cargo test
```

Tests include validation of DNS resolution, address formatting, and data structure operations.

## Dependencies

- `tokio`: Async runtime
- `trust-dns-resolver`: DNS resolution with explicit A/AAAA record support
- `serde`: Serialization support for data structures
- `thiserror`: Error handling
- `tracing`: Logging
- `rand`: Peer list randomization 