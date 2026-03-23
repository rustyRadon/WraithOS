use super::WraithCommand;
use anyhow::Result;
use async_trait::async_trait;
use sentinel_core::engine::SentinelNode;
use std::sync::Arc;

pub struct PeersCommand;

#[async_trait]
impl WraithCommand for PeersCommand {
    async fn execute(&self, node: Arc<SentinelNode>, _args: Vec<String>) -> Result<()> {
        super::print_spectral_header("Active Peer Handshakes");
        
        if node.peers.is_empty() {
            println!("[Scanning...] No active peer connections in this sector.");
        } else {
            for entry in node.peers.iter() {
                let peer = entry.value();
                println!("ID: {} | Node: {} | Status: Entangled", 
                    &peer.node_id[..12], 
                    peer.node_name
                );
            }
        }
        Ok(())
    }
}