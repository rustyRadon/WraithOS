use super::WraithCommand;
use anyhow::Result;
use async_trait::async_trait;
use sentinel_core::engine::SentinelNode;
use std::sync::Arc;

pub struct RequestCommand;

#[async_trait]
impl WraithCommand for RequestCommand {
    async fn execute(&self, node: Arc<SentinelNode>, args: Vec<String>) -> Result<()> {
        if args.is_empty() {
            println!("Usage: request <node_public_key>");
            return Ok(());
        }

        let target_id = args[0].clone();
        println!("⛧ Initiating spectral search for: {}...", &target_id[..12]);

        // ts runs in a clone so the CLI stays responsive while we wait for the Signaler
        let node_clone = Arc::clone(&node);
        tokio::spawn(async move {
            match node_clone.request_ghost_by_id(target_id.clone()).await {
                Ok(_) => println!("\n[✓] Veil pierced. Connection established with Ghost {}", &target_id[..8]),
                Err(e) => println!("\n[✗] Discovery failed for {}: {}", &target_id[..8], e),
            }
        });

        Ok(())
    }
}