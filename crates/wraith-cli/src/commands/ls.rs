use super::WraithCommand;
use anyhow::Result;
use async_trait::async_trait;
use sentinel_core::engine::SentinelNode;
use std::sync::Arc;

pub struct LsCommand;

#[async_trait]
impl WraithCommand for LsCommand {
    async fn execute(&self, node: Arc<SentinelNode>, _args: Vec<String>) -> Result<()> {
        super::print_spectral_header("Local Spectral Library");
        
        match node.get_local_library() {
            Ok(files) => {
                if files.is_empty() {
                    println!("[Empty Void] No files manifested in this sector.");
                } else {
                    for f in files {
                        println!("ID: {} | Name: {} | Chunks: {}", 
                            &f.id.to_string()[..8], f.name, f.total_chunks);
                    }
                }
            }
            Err(e) => println!("✗ Error reading library: {}", e),
        }
        Ok(())
    }
}