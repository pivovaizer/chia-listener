use dig_dns_discovery::DnsDiscovery;
pub use dig_dns_discovery::{DiscoveryResult, DnsDiscoveryError, PeerAddress};

/// Canonical Rust DNS discovery client (core, N-API free).
pub struct DnsDiscoveryClient {
    inner: DnsDiscovery,
}

impl DnsDiscoveryClient {
    /// Create a new DNS discovery client using default resolver configuration.
    pub async fn new() -> Result<Self, DnsDiscoveryError> {
        let inner = DnsDiscovery::new().await?;
        Ok(Self { inner })
    }

    pub async fn discover_mainnet_peers(&self) -> Result<DiscoveryResult, DnsDiscoveryError> {
        self.inner.discover_mainnet_peers().await
    }

    pub async fn discover_testnet11_peers(&self) -> Result<DiscoveryResult, DnsDiscoveryError> {
        self.inner.discover_testnet11_peers().await
    }

    pub async fn discover_peers(
        &self,
        introducers: &[String],
        default_port: u16,
    ) -> Result<DiscoveryResult, DnsDiscoveryError> {
        let introducers_refs: Vec<&str> = introducers.iter().map(|s| s.as_str()).collect();
        self.inner
            .discover_peers(&introducers_refs, default_port)
            .await
    }

    pub async fn resolve_ipv4(
        &self,
        hostname: &str,
    ) -> Result<Vec<std::net::Ipv4Addr>, DnsDiscoveryError> {
        self.inner.resolve_ipv4(hostname).await
    }

    pub async fn resolve_ipv6(
        &self,
        hostname: &str,
    ) -> Result<Vec<std::net::Ipv6Addr>, DnsDiscoveryError> {
        self.inner.resolve_ipv6(hostname).await
    }

    pub async fn resolve_both(
        &self,
        hostname: &str,
        port: u16,
    ) -> Result<DiscoveryResult, DnsDiscoveryError> {
        let ipv4 = self.inner.resolve_ipv4(hostname).await?;
        let ipv6 = self.inner.resolve_ipv6(hostname).await?;

        let mut result = DiscoveryResult::new();
        for addr in ipv4 {
            result.add_ipv4(addr, port);
        }
        for addr in ipv6 {
            result.add_ipv6(addr, port);
        }
        Ok(result)
    }
}
