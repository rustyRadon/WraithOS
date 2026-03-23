use super::WraithCommand;
use anyhow::Result;
use async_trait::async_trait;
use sentinel_core::engine::SentinelNode;
use std::sync::Arc;

pub struct UploadCommand;

#[async_trait]
impl WraithCommand for UploadCommand {
    async fn execute(&self, node: Arc<SentinelNode>, args: Vec<String>) -> Result<()> {
        if args.len() < 1 {
            println!("Usage: upload <path_to_file>");
            return Ok(());
        }

        let path = &args[0];
        println!("⛧ Slicing {} into the void...", path);

        let mut key = [0u8; 32];
        let pk_bytes = node.identity.public_key_bytes();
        let len = pk_bytes.len().min(32);
        key[..len].copy_from_slice(&pk_bytes[..len]);

        match wraith_fs::slice_it_up(path, &key) {
            Ok((file_id, chunks)) => {
                node.ingest_file(path, file_id.clone(), chunks)?;
                println!("✓ File Manifested successfully.");
                println!("Manifest ID: {}", file_id);
            }
            Err(e) => println!("✗ Failed to manifest file: {}", e),
        }
        Ok(())
    }
}