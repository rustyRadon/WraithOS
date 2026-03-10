use super::WraithCommand;
use anyhow::Result;
use async_trait::async_trait;
use sentinel_core::engine::SentinelNode;
use std::sync::Arc;

pub struct IdCommand;

#[async_trait]
impl WraithCommand for IdCommand {
    async fn execute(&self, node: Arc<SentinelNode>, _args: Vec<String>) -> Result<()> {
        super::print_spectral_header("Spectral Identity");
        
        println!("Public Node ID: {}", node.identity.node_id());
        
        if let Some(phrase) = &node.identity.mnemonic_phrase {
            println!("Recovery Phrase: {}", phrase);
        }

        let addr_lock = node.public_addr.read().await;
        if let Some(ip) = *addr_lock {
            println!("Manifested IP: {}", ip);
        } else {
            println!("IP Status: Concealed (STUN discovery in progress)");
        }

        Ok(())
    }
}