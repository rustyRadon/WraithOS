use super::{WraithCommand, print_spectral_header};
use anyhow::{Result, Context};
use async_trait::async_trait;
use sentinel_core::engine::SentinelNode;
use std::sync::Arc;
use std::fs::File;
use std::io::Write;

pub struct ReifyCommand;

#[async_trait]
impl WraithCommand for ReifyCommand {
    async fn execute(&self, node: Arc<SentinelNode>, args: Vec<String>) -> Result<()> {
        if args.is_empty() {
            println!("Usage: reify <file_id>");
            return Ok(());
        }

        let file_id = &args[0];
        print_spectral_header("Reifying Spectral Object");

        let manifest = node.get_file_manifest(file_id)
            .context("Failed to find file manifest in the void.")?;

        println!("⛧ File Found: {}", manifest.name);
        println!("⛧ Chunks to Reconstitute: {}", manifest.total_chunks);

        let output_path = format!("restored_{}", manifest.name);
        let mut output_file = File::create(&output_path)
            .context("Failed to create the physical container for the file.")?;

        let mut key = [0u8; 32];
        let pk_bytes = node.identity.public_key_bytes();
        let len = pk_bytes.len().min(32);
        key[..len].copy_from_slice(&pk_bytes[..len]);

        for (index, chunk_hash) in manifest.chunks.iter().enumerate() {
            print!("\r⛧ Reconstituting chunk {}/{}...", index + 1, manifest.total_chunks);
            std::io::stdout().flush().unwrap();

            let encrypted_data = node.load_chunk(chunk_hash)
                .await
                .context(format!("Missing chunk: {}", chunk_hash))?;

            let decrypted_data = wraith_fs::decrypt_chunk(&encrypted_data, &key, index as u32)
                .context("Decryption failed. Identity mismatch?")?;

            output_file.write_all(&decrypted_data)?;
        }

        println!("\n\n✓ Manifestation Complete!");
        println!("✓ Physical file restored to: {}", output_path);
        
        Ok(())
    }
}