use crate::dns_discovery::DnsDiscoveryClient;
use crate::error::ChiaError;
use crate::peer_pool::ChiaPeerPool;
use crate::types::{BlockListenerConfig, Event};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct BlockListener {
    pool: Arc<ChiaPeerPool>,
    tx: broadcast::Sender<Event>,
    cancel: CancellationToken,
    dispatcher: Mutex<Option<JoinHandle<()>>>,
    auto_reconnect_started: AtomicBool,
    config: BlockListenerConfig,
}

impl BlockListener {
    pub fn new(config: BlockListenerConfig) -> Result<Self, ChiaError> {
        let (tx, _rx) = broadcast::channel(config.buffer);

        // Internal event sink between pool -> listener dispatcher
        let (sink_tx, mut sink_rx) = mpsc::channel::<Event>(config.buffer);
        let cancel = CancellationToken::new();

        // Pool constructed with event sink and shared cancellation
        let pool = Arc::new(ChiaPeerPool::new_with_event_sink(sink_tx, cancel.clone()));

        // Dispatcher: forwards from sink to broadcast without blocking pool
        let tx_clone = tx.clone();
        let cancel_clone = cancel.clone();
        let dispatcher = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_clone.cancelled() => {
                        break;
                    }
                    maybe = sink_rx.recv() => {
                        match maybe {
                            Some(ev) => { let _ = tx_clone.send(ev); }
                            None => break,
                        }
                    }
                }
            }
        });

        Ok(Self {
            pool,
            tx,
            cancel,
            dispatcher: Mutex::new(Some(dispatcher)),
            auto_reconnect_started: AtomicBool::new(false),
            config,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        if self.config.auto_reconnect {
            self.start_auto_reconnect_task();
        }
        self.tx.subscribe()
    }

    fn start_auto_reconnect_task(&self) {
        if self
            .auto_reconnect_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let cancel = self.cancel.clone();
        let pool = self.pool.clone();
        let mut rx = self.tx.subscribe();
        let config = self.config.clone();
        let tx = self.tx.clone();

        tokio::spawn(async move {
            // Create DNS client once
            let dns_client = match DnsDiscoveryClient::new().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("auto_reconnect: failed to create DNS client: {e}");
                    return;
                }
            };

            // helper: top-up peers until we reach target_peers count.
            // Returns the number of new peers successfully connected.
            async fn fill_peers(
                dns_client: &DnsDiscoveryClient,
                pool: &ChiaPeerPool,
                config: &BlockListenerConfig,
                cancel: &CancellationToken,
            ) -> Result<usize, ChiaError> {
                if cancel.is_cancelled() {
                    return Err(ChiaError::Other("shutting down".into()));
                }

                let current = pool
                    .get_connected_peers()
                    .await
                    .unwrap_or_default()
                    .len();
                let needed = config.target_peers.saturating_sub(current);
                if needed == 0 {
                    return Ok(0);
                }

                let discovery_res = match config.network_id.as_str() {
                    "mainnet" => dns_client.discover_mainnet_peers().await,
                    "testnet11" | "testnet" => dns_client.discover_testnet11_peers().await,
                    _ => dns_client.discover_mainnet_peers().await,
                };

                let mut discovery = match discovery_res {
                    Ok(d) => d,
                    Err(e) => {
                        return Err(ChiaError::Connection(format!("DNS discovery failed: {e}")))
                    }
                };

                discovery.shuffle();

                // Collect already-connected peer ids to skip duplicates
                let connected: std::collections::HashSet<String> = pool
                    .get_connected_peers()
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .collect();

                let mut added = 0usize;

                for peer in discovery
                    .ipv4_peers
                    .iter()
                    .chain(discovery.ipv6_peers.iter())
                {
                    if cancel.is_cancelled() {
                        return Err(ChiaError::Other("shutting down".into()));
                    }
                    if added >= needed {
                        break;
                    }
                    let host = peer.host.to_string();
                    let port = peer.port;
                    let peer_id = format!("{host}:{port}");
                    if connected.contains(&peer_id) {
                        continue;
                    }
                    match pool
                        .add_peer(host.clone(), port, config.network_id.clone())
                        .await
                    {
                        Ok(_) => {
                            tracing::info!("auto_reconnect: connected to {host}:{port}");
                            added += 1;
                        }
                        Err(e) => {
                            tracing::warn!("auto_reconnect: failed to add peer {host}:{port}: {e}");
                        }
                    }
                }

                if added > 0 {
                    Ok(added)
                } else {
                    Err(ChiaError::Connection(
                        "Could not connect to any new peers".into(),
                    ))
                }
            }

            // Initial fill: connect up to target_peers
            let _ = fill_peers(&dns_client, &pool, &config, &cancel).await;

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    evt = rx.recv() => {
                        match evt {
                            Ok(Event::PeerDisconnected(_)) => {
                                // Top-up: if below target, try to add more peers
                                let current = pool.get_connected_peers().await.unwrap_or_default().len();
                                if current < config.target_peers {
                                    let mut retry: u32 = 0;
                                    while retry < config.max_auto_reconnect_retries {
                                        if cancel.is_cancelled() { break; }
                                        match fill_peers(&dns_client, &pool, &config, &cancel).await {
                                            Ok(_) => { break; }
                                            Err(e) => {
                                                retry += 1;
                                                tracing::warn!("auto_reconnect retry {}/{} failed: {}", retry, config.max_auto_reconnect_retries, e);
                                                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                            }
                                        }
                                    }
                                    if retry >= config.max_auto_reconnect_retries {
                                        tracing::error!("auto_reconnect: exceeded max retries ({}); giving up until next disconnect", config.max_auto_reconnect_retries);
                                        let _ = tx.send(Event::PeerDisconnected(crate::types::PeerDisconnectedEvent {
                                            peer_id: "auto-reconnect".to_string(),
                                            host: "".to_string(),
                                            port: config.default_port as u32,
                                            message: Some("auto_reconnect failed to find peers".to_string()),
                                        }));
                                    }
                                }
                            }
                            Ok(Event::PeerConnected(_)) => {}
                            _ => {}
                        }
                    }
                }
            }
        });
    }

    pub async fn add_peer(
        &self,
        host: String,
        port: u16,
        network_id: String,
    ) -> Result<String, ChiaError> {
        if self.cancel.is_cancelled() {
            return Err(ChiaError::Other("shutting down".into()));
        }
        self.pool.add_peer(host, port, network_id).await
    }

    pub async fn remove_peer(&self, peer_id: String) -> Result<bool, ChiaError> {
        if self.cancel.is_cancelled() {
            return Err(ChiaError::Other("shutting down".into()));
        }
        self.pool.remove_peer(peer_id).await
    }

    pub async fn get_connected_peers(&self) -> Result<Vec<String>, ChiaError> {
        if self.cancel.is_cancelled() {
            return Err(ChiaError::Other("shutting down".into()));
        }
        self.pool.get_connected_peers().await
    }

    pub async fn get_highest_peak(&self) -> Option<u32> {
        self.pool.get_highest_peak().await
    }

    pub async fn get_block_by_height(
        &self,
        height: u64,
    ) -> Result<crate::types::BlockReceivedEvent, ChiaError> {
        if self.cancel.is_cancelled() {
            return Err(ChiaError::Other("shutting down".into()));
        }
        self.pool.get_block_by_height(height).await
    }

    // Structured shutdown
    pub async fn shutdown(&self) -> Result<(), ChiaError> {
        // Signal cancellation quickly and initiate pool shutdown
        self.cancel.cancel();
        self.pool.shutdown().await
    }

    pub async fn shutdown_and_wait(&self) -> Result<(), ChiaError> {
        // First signal shutdown
        self.shutdown().await?;

        // Take and await dispatcher task cooperatively (no abort)
        if let Some(handle) = self.dispatcher.lock().ok().and_then(|mut g| g.take()) {
            let _ = handle.await;
        }

        // Await pool tasks
        self.pool.shutdown_and_wait().await
    }
}

impl Drop for BlockListener {
    fn drop(&mut self) {
        // Ensure background tasks are signalled to stop if user drops without explicit shutdown
        self.cancel.cancel();
        // Do not await here; Drop must be non-blocking.
    }
}
