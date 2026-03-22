use crate::{error::ChiaError, tls};
use chia_generator_parser::{parser::BlockParser, types::ParsedBlock};
use chia_protocol::{
    FullBlock, Handshake as ChiaHandshake, NewPeakWallet, NodeType, ProtocolMessageTypes,
    RequestBlock, RespondBlock,
};
use chia_traits::Streamable;
use futures_util::{SinkExt, StreamExt};
use std::net::IpAddr;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async_tls_with_config, tungstenite::Message as WsMessage, Connector, MaybeTlsStream,
    WebSocketStream,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};

type WebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Clone)]
pub struct PeerConnection {
    host: String,
    port: u16,
    network_id: String,
}

#[derive(Clone, Debug)]
pub enum StreamEvent {
    ParsedBlock(ParsedBlock),
    NewPeak(u32),
}

impl PeerConnection {
    pub fn new(host: String, port: u16, network_id: String) -> Self {
        Self {
            host,
            port,
            network_id,
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn connect(&self) -> Result<WebSocket, ChiaError> {
        info!("Connecting to peer at {}:{}", self.host, self.port);

        // Load or generate certificates
        let cert = tls::load_or_generate_cert()?;
        let tls_connector = tls::create_tls_connector(&cert)?;
        let connector = Connector::NativeTls(tls_connector);

        // Check if the host is an IPv6 address and format accordingly
        // IPv6 addresses need to be wrapped in brackets when used in URLs
        let host_formatted = match self.host.parse::<IpAddr>() {
            Ok(IpAddr::V6(_)) => {
                // This is an IPv6 address, wrap it in brackets
                format!("[{}]", self.host)
            }
            _ => {
                // IPv4 address or hostname, use as-is
                self.host.clone()
            }
        };

        let url = format!("wss://{}:{}/ws", host_formatted, self.port);
        debug!("WebSocket URL: {}", url);

        let (ws_stream, _) = connect_async_tls_with_config(&url, None, false, Some(connector))
            .await
            .map_err(|e| ChiaError::WebSocket(Box::new(e)))?;

        info!("WebSocket connection established to {}", self.host);
        Ok(ws_stream)
    }

    pub async fn handshake(&self, ws_stream: &mut WebSocket) -> Result<(), ChiaError> {
        info!("Performing Chia handshake with {}", self.host);

        // Send our handshake - matching SDK exactly
        let handshake = ChiaHandshake {
            network_id: self.network_id.clone(),
            protocol_version: "0.0.37".to_string(),
            software_version: "0.0.0".to_string(),
            server_port: 0,              // 0 for wallet clients
            node_type: NodeType::Wallet, // Connect as wallet
            capabilities: vec![
                (1, "1".to_string()), // BASE
                (2, "1".to_string()), // BLOCK_HEADERS
                (3, "1".to_string()), // RATE_LIMITS_V2
            ],
        };

        // Serialize and send handshake
        let handshake_bytes = handshake
            .to_bytes()
            .map_err(|e| ChiaError::Protocol(e.to_string()))?;

        let message = chia_protocol::Message {
            msg_type: ProtocolMessageTypes::Handshake,
            id: None,
            data: handshake_bytes.into(),
        };

        let message_bytes = message
            .to_bytes()
            .map_err(|e| ChiaError::Protocol(e.to_string()))?;

        ws_stream
            .send(WsMessage::Binary(message_bytes))
            .await
            .map_err(|e| ChiaError::WebSocket(Box::new(e)))?;

        // Wait for peer's handshake
        if let Some(msg) = ws_stream.next().await {
            match msg {
                Ok(WsMessage::Binary(data)) => {
                    let response = chia_protocol::Message::from_bytes(&data)
                        .map_err(|e| ChiaError::Protocol(e.to_string()))?;

                    if response.msg_type == ProtocolMessageTypes::Handshake {
                        // Parse and validate peer's handshake
                        let peer_handshake = ChiaHandshake::from_bytes(&response.data)
                            .map_err(|e| ChiaError::Protocol(e.to_string()))?;

                        if peer_handshake.node_type != NodeType::FullNode {
                            return Err(ChiaError::Protocol(format!(
                                "Expected FullNode, got {:?}",
                                peer_handshake.node_type
                            )));
                        }

                        if peer_handshake.network_id != self.network_id {
                            return Err(ChiaError::Protocol(format!(
                                "Network ID mismatch: expected {}, got {}",
                                self.network_id, peer_handshake.network_id
                            )));
                        }

                        info!(
                            "Handshake successful with {} (protocol: {})",
                            self.host, peer_handshake.protocol_version
                        );
                        Ok(())
                    } else {
                        Err(ChiaError::Protocol(format!(
                            "Expected handshake, got message type {:?}",
                            response.msg_type
                        )))
                    }
                }
                Ok(WsMessage::Close(_)) => Err(ChiaError::Connection(
                    "Peer closed connection during handshake".to_string(),
                )),
                Ok(_) => Err(ChiaError::Protocol("Unexpected message type".to_string())),
                Err(e) => Err(ChiaError::WebSocket(Box::new(e))),
            }
        } else {
            Err(ChiaError::Connection(
                "Connection closed during handshake".to_string(),
            ))
        }
    }

    pub async fn listen_for_blocks(
        mut ws_stream: WebSocket,
        event_sender: mpsc::Sender<StreamEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ChiaError> {
        info!("Listening for blocks and messages");

        loop {
            let next_msg = tokio::select! {
                _ = cancel.cancelled() => break,
                msg = ws_stream.next() => msg,
            };

            let Some(msg) = next_msg else {
                break;
            };
            match msg {
                Ok(WsMessage::Binary(data)) => {
                    match chia_protocol::Message::from_bytes(&data) {
                        Ok(message) => {
                            trace!("Received message type: {:?}", message.msg_type);

                            match message.msg_type {
                                ProtocolMessageTypes::NewPeakWallet => {
                                    if let Ok(new_peak) = NewPeakWallet::from_bytes(&message.data) {
                                        debug!(
                                            "New peak at height {} from wallet perspective",
                                            new_peak.height
                                        );

                                        // Emit new peak notification (best-effort)
                                        if let Err(e) = event_sender
                                            .try_send(StreamEvent::NewPeak(new_peak.height))
                                        {
                                            warn!("Failed to queue new peak event: {}", e);
                                        }

                                        // Request the full block
                                        let request = RequestBlock {
                                            height: new_peak.height,
                                            include_transaction_block: true,
                                        };

                                        if let Ok(request_bytes) = request.to_bytes() {
                                            let request_msg = chia_protocol::Message {
                                                msg_type: ProtocolMessageTypes::RequestBlock,
                                                id: Some(1), // Add request ID
                                                data: request_bytes.into(),
                                            };

                                            if let Ok(msg_bytes) = request_msg.to_bytes() {
                                                if let Err(e) = ws_stream
                                                    .send(WsMessage::Binary(msg_bytes))
                                                    .await
                                                {
                                                    error!("Failed to request block: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }

                                ProtocolMessageTypes::NewPeak => {
                                    // This is for full nodes - we might see this too
                                    trace!("Received NewPeak (full node message)");
                                }

                                ProtocolMessageTypes::RespondBlock => {
                                    match RespondBlock::from_bytes(&message.data) {
                                        Ok(respond_block) => {
                                            let block = respond_block.block;
                                            debug!(
                                                "Received block at height {}",
                                                block.reward_chain_block.height
                                            );

                                            // Parse the block using chia-generator-parser
                                            match Self::parse_block(block).await {
                                                Ok(parsed_block) => {
                                                    if let Err(e) = event_sender
                                                        .send(StreamEvent::ParsedBlock(
                                                            parsed_block,
                                                        ))
                                                        .await
                                                    {
                                                        error!(
                                                            "Failed to send parsed block through channel: {}",
                                                            e
                                                        );
                                                        break;
                                                    }
                                                }
                                                Err(e) => {
                                                    error!("Failed to parse block: {}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to parse RespondBlock: {}", e);
                                        }
                                    }
                                }

                                ProtocolMessageTypes::CoinStateUpdate => {
                                    trace!("Received coin state update");
                                }

                                _ => {
                                    trace!("Received other message type: {:?}", message.msg_type);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse message: {}", e);
                        }
                    }
                }
                Ok(WsMessage::Close(frame)) => {
                    info!("Peer closed connection: {:?}", frame);
                    break;
                }
                Ok(WsMessage::Ping(data)) => {
                    // Respond to ping
                    if let Err(e) = ws_stream.send(WsMessage::Pong(data)).await {
                        error!("Failed to send pong: {}", e);
                    }
                }
                Ok(_) => {
                    // Ignore other message types
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    return Err(ChiaError::WebSocket(Box::new(e)));
                }
            }
        }

        info!("Connection closed");
        Ok(())
    }

    /// Parse a FullBlock using chia-generator-parser
    async fn parse_block(block: FullBlock) -> Result<ParsedBlock, ChiaError> {
        debug!(
            "Parsing block at height {}",
            block.reward_chain_block.height
        );

        // Use chia-generator-parser to parse the block directly
        let parser = BlockParser::new();
        let parsed_block = parser
            .parse_full_block(&block)
            .map_err(|e| ChiaError::Protocol(e.to_string()))?;

        debug!(
            "Parsed block {}: {} coin additions, {} coin removals, {} coin spends, generator: {}",
            parsed_block.height,
            parsed_block.coin_additions.len(),
            parsed_block.coin_removals.len(),
            parsed_block.coin_spends.len(),
            parsed_block.has_transactions_generator
        );

        Ok(parsed_block)
    }

    pub async fn request_block_by_height(
        &self,
        height: u64,
        ws_stream: &mut WebSocket,
    ) -> Result<FullBlock, ChiaError> {
        debug!("Requesting block at height {}", height);

        let request = RequestBlock {
            height: height as u32,
            include_transaction_block: true,
        };

        let request_bytes = request
            .to_bytes()
            .map_err(|e| ChiaError::Protocol(e.to_string()))?;

        let request_msg = chia_protocol::Message {
            msg_type: ProtocolMessageTypes::RequestBlock,
            id: Some(1), // Add request ID
            data: request_bytes.into(),
        };

        let request_bytes = request_msg
            .to_bytes()
            .map_err(|e| ChiaError::Protocol(e.to_string()))?;

        ws_stream
            .send(WsMessage::Binary(request_bytes))
            .await
            .map_err(|e| ChiaError::WebSocket(Box::new(e)))?;

        // Wait for the response, handling other messages in between
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 100; // Prevent infinite loops

        while attempts < MAX_ATTEMPTS {
            attempts += 1;

            if let Some(msg) = ws_stream.next().await {
                match msg {
                    Ok(WsMessage::Binary(data)) => {
                        match chia_protocol::Message::from_bytes(&data) {
                            Ok(response) => {
                                trace!(
                                    "Received message type: {:?} while waiting for block",
                                    response.msg_type
                                );

                                match response.msg_type {
                                    ProtocolMessageTypes::RespondBlock => {
                                        match RespondBlock::from_bytes(&response.data) {
                                            Ok(respond_block) => {
                                                let block = respond_block.block;
                                                debug!(
                                                    "Received block at height {} - transactions_generator: {} bytes, has_foliage_transaction_block: {}",
                                                    block.reward_chain_block.height,
                                                    block.transactions_generator.as_ref().map(|g| g.len()).unwrap_or(0),
                                                    block.foliage_transaction_block.is_some()
                                                );
                                                return Ok(block);
                                            }
                                            Err(e) => {
                                                error!("Failed to parse RespondBlock: {}", e);
                                                return Err(ChiaError::Protocol(e.to_string()));
                                            }
                                        }
                                    }
                                    ProtocolMessageTypes::RejectBlock => {
                                        error!("Block request rejected by peer");
                                        return Err(ChiaError::Protocol(
                                            "Block request rejected".to_string(),
                                        ));
                                    }
                                    ProtocolMessageTypes::NewPeakWallet => {
                                        // Just log and continue waiting for our response
                                        if let Ok(new_peak) =
                                            NewPeakWallet::from_bytes(&response.data)
                                        {
                                            trace!("Received NewPeakWallet at height {} while waiting for block", new_peak.height);
                                        }
                                        continue;
                                    }
                                    ProtocolMessageTypes::CoinStateUpdate => {
                                        trace!("Received CoinStateUpdate while waiting for block");
                                        continue;
                                    }
                                    _ => {
                                        trace!("Received other message type while waiting for block: {:?}", response.msg_type);
                                        continue;
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse message while waiting for block: {}", e);
                                continue;
                            }
                        }
                    }
                    Ok(WsMessage::Close(_)) => {
                        error!("Peer closed connection during block request");
                        return Err(ChiaError::Connection(
                            "Peer closed connection during block request".to_string(),
                        ));
                    }
                    Ok(WsMessage::Ping(data)) => {
                        // Respond to ping
                        if let Err(e) = ws_stream.send(WsMessage::Pong(data)).await {
                            error!("Failed to send pong: {}", e);
                        }
                        continue;
                    }
                    Ok(_) => {
                        trace!("Unexpected WebSocket message type during block request");
                        continue;
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        return Err(ChiaError::WebSocket(Box::new(e)));
                    }
                }
            } else {
                error!("Connection closed during block request");
                return Err(ChiaError::Connection(
                    "Connection closed during block request".to_string(),
                ));
            }
        }

        error!(
            "Timeout waiting for block response after {} attempts",
            MAX_ATTEMPTS
        );
        Err(ChiaError::Protocol(
            "Timeout waiting for block response".to_string(),
        ))
    }
}
