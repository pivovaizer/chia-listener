use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use thiserror::Error;
use tracing::{debug, error, trace, warn};
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use trust_dns_resolver::TokioAsyncResolver;

#[derive(Error, Debug)]
pub enum DnsDiscoveryError {
    #[error("DNS resolution failed: {0}")]
    ResolutionFailed(String),
    #[error("No peers found from any introducer")]
    NoPeersFound,
    #[error("Resolver creation failed: {0}")]
    ResolverCreationFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAddress {
    pub host: IpAddr,
    pub port: u16,
    pub is_ipv6: bool,
}

impl PeerAddress {
    pub fn new(host: IpAddr, port: u16) -> Self {
        let is_ipv6 = matches!(host, IpAddr::V6(_));
        Self {
            host,
            port,
            is_ipv6,
        }
    }

    /// Format address for display (IPv6 needs brackets)
    pub fn display_address(&self) -> String {
        if self.is_ipv6 {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryResult {
    pub ipv4_peers: Vec<PeerAddress>,
    pub ipv6_peers: Vec<PeerAddress>,
    pub total_count: usize,
}

impl DiscoveryResult {
    pub fn new() -> Self {
        Self {
            ipv4_peers: Vec::new(),
            ipv6_peers: Vec::new(),
            total_count: 0,
        }
    }

    pub fn add_ipv4(&mut self, addr: Ipv4Addr, port: u16) {
        self.ipv4_peers
            .push(PeerAddress::new(IpAddr::V4(addr), port));
        self.total_count += 1;
    }

    pub fn add_ipv6(&mut self, addr: Ipv6Addr, port: u16) {
        self.ipv6_peers
            .push(PeerAddress::new(IpAddr::V6(addr), port));
        self.total_count += 1;
    }

    pub fn merge(&mut self, other: DiscoveryResult) {
        self.ipv4_peers.extend(other.ipv4_peers);
        self.ipv6_peers.extend(other.ipv6_peers);
        self.total_count = self.ipv4_peers.len() + self.ipv6_peers.len();
    }

    pub fn shuffle(&mut self) {
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        self.ipv4_peers.shuffle(&mut rng);
        self.ipv6_peers.shuffle(&mut rng);
    }
}

impl Default for DiscoveryResult {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DnsDiscovery {
    resolver: TokioAsyncResolver,
}

impl DnsDiscovery {
    pub async fn new() -> Result<Self, DnsDiscoveryError> {
        let resolver =
            TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());

        Ok(Self { resolver })
    }

    /// Discover peers for Chia mainnet
    pub async fn discover_mainnet_peers(&self) -> Result<DiscoveryResult, DnsDiscoveryError> {
        let introducers = vec![
            "dns-introducer.chia.net",
            "seeder.dexie.space",
            "chia.hoffmang.com",
        ];

        self.discover_peers(&introducers, 8444).await
    }

    /// Discover peers for Chia testnet11
    pub async fn discover_testnet11_peers(&self) -> Result<DiscoveryResult, DnsDiscoveryError> {
        let introducers = vec!["dns-introducer-testnet11.chia.net"];

        self.discover_peers(&introducers, 58444).await
    }

    /// Generic peer discovery for any list of introducers
    pub async fn discover_peers(
        &self,
        introducers: &[&str],
        default_port: u16,
    ) -> Result<DiscoveryResult, DnsDiscoveryError> {
        debug!(
            "Starting DNS discovery for {} introducers",
            introducers.len()
        );

        let mut result = DiscoveryResult::new();

        for introducer in introducers {
            debug!("Resolving introducer: {}", introducer);

            // Resolve IPv4 addresses (A records)
            match self.resolve_ipv4(introducer).await {
                Ok(ipv4_addrs) => {
                    trace!(
                        "Found {} IPv4 addresses for {}",
                        ipv4_addrs.len(),
                        introducer
                    );
                    for addr in ipv4_addrs {
                        result.add_ipv4(addr, default_port);
                    }
                }
                Err(e) => {
                    warn!("Failed to resolve IPv4 for {}: {}", introducer, e);
                }
            }

            // Resolve IPv6 addresses (AAAA records)
            match self.resolve_ipv6(introducer).await {
                Ok(ipv6_addrs) => {
                    trace!(
                        "Found {} IPv6 addresses for {}",
                        ipv6_addrs.len(),
                        introducer
                    );
                    for addr in ipv6_addrs {
                        result.add_ipv6(addr, default_port);
                    }
                }
                Err(e) => {
                    warn!("Failed to resolve IPv6 for {}: {}", introducer, e);
                }
            }
        }

        if result.total_count == 0 {
            return Err(DnsDiscoveryError::NoPeersFound);
        }

        // Shuffle for randomness
        result.shuffle();

        debug!(
            "DNS discovery completed: {} IPv4 peers, {} IPv6 peers, {} total",
            result.ipv4_peers.len(),
            result.ipv6_peers.len(),
            result.total_count
        );

        Ok(result)
    }

    /// Resolve IPv4 addresses (A records) for a hostname
    pub async fn resolve_ipv4(&self, hostname: &str) -> Result<Vec<Ipv4Addr>, DnsDiscoveryError> {
        trace!("Performing A record lookup for {}", hostname);

        match self.resolver.ipv4_lookup(hostname).await {
            Ok(lookup) => {
                let addrs: Vec<Ipv4Addr> = lookup.iter().map(|record| record.0).collect();
                trace!(
                    "A record lookup for {} returned {} addresses",
                    hostname,
                    addrs.len()
                );
                Ok(addrs)
            }
            Err(e) => {
                error!("A record lookup failed for {}: {}", hostname, e);
                Err(DnsDiscoveryError::ResolutionFailed(format!(
                    "IPv4 lookup for {}: {}",
                    hostname, e
                )))
            }
        }
    }

    /// Resolve IPv6 addresses (AAAA records) for a hostname
    pub async fn resolve_ipv6(&self, hostname: &str) -> Result<Vec<Ipv6Addr>, DnsDiscoveryError> {
        trace!("Performing AAAA record lookup for {}", hostname);

        match self.resolver.ipv6_lookup(hostname).await {
            Ok(lookup) => {
                let addrs: Vec<Ipv6Addr> = lookup.iter().map(|record| record.0).collect();
                trace!(
                    "AAAA record lookup for {} returned {} addresses",
                    hostname,
                    addrs.len()
                );
                Ok(addrs)
            }
            Err(e) => {
                error!("AAAA record lookup failed for {}: {}", hostname, e);
                Err(DnsDiscoveryError::ResolutionFailed(format!(
                    "IPv6 lookup for {}: {}",
                    hostname, e
                )))
            }
        }
    }

    /// Resolve both IPv4 and IPv6 addresses for a single hostname
    pub async fn resolve_both(
        &self,
        hostname: &str,
        port: u16,
    ) -> Result<DiscoveryResult, DnsDiscoveryError> {
        let mut result = DiscoveryResult::new();

        // Try IPv4
        if let Ok(ipv4_addrs) = self.resolve_ipv4(hostname).await {
            for addr in ipv4_addrs {
                result.add_ipv4(addr, port);
            }
        }

        // Try IPv6
        if let Ok(ipv6_addrs) = self.resolve_ipv6(hostname).await {
            for addr in ipv6_addrs {
                result.add_ipv6(addr, port);
            }
        }

        if result.total_count == 0 {
            return Err(DnsDiscoveryError::ResolutionFailed(format!(
                "No addresses found for {}",
                hostname
            )));
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dns_discovery_creation() {
        let discovery = DnsDiscovery::new().await;
        assert!(discovery.is_ok());
    }

    #[tokio::test]
    async fn test_ipv4_resolution() {
        let discovery = DnsDiscovery::new().await.unwrap();

        // Test with a well-known hostname that should have IPv4
        let result = discovery.resolve_ipv4("dns.google").await;
        assert!(result.is_ok());
        let addrs = result.unwrap();
        assert!(!addrs.is_empty());
    }

    #[tokio::test]
    async fn test_ipv6_resolution() {
        let discovery = DnsDiscovery::new().await.unwrap();

        // Test with a well-known hostname that should have IPv6
        let result = discovery.resolve_ipv6("dns.google").await;
        // Note: IPv6 may not be available in all test environments, so we don't assert success
        println!("IPv6 resolution result: {:?}", result);
    }

    #[tokio::test]
    async fn test_peer_address_formatting() {
        let ipv4_peer = PeerAddress::new(IpAddr::V4("192.168.1.1".parse().unwrap()), 8444);
        assert_eq!(ipv4_peer.display_address(), "192.168.1.1:8444");
        assert!(!ipv4_peer.is_ipv6);

        let ipv6_peer = PeerAddress::new(IpAddr::V6("2001:db8::1".parse().unwrap()), 8444);
        assert_eq!(ipv6_peer.display_address(), "[2001:db8::1]:8444");
        assert!(ipv6_peer.is_ipv6);
    }

    #[tokio::test]
    async fn test_discovery_result_operations() {
        let mut result = DiscoveryResult::new();

        result.add_ipv4("192.168.1.1".parse().unwrap(), 8444);
        result.add_ipv6("2001:db8::1".parse().unwrap(), 8444);

        assert_eq!(result.ipv4_peers.len(), 1);
        assert_eq!(result.ipv6_peers.len(), 1);
        assert_eq!(result.total_count, 2);
    }
}
