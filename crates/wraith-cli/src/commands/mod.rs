use anyhow::Result;
use async_trait::async_trait;
use sentinel_core::engine::SentinelNode;
use std::sync::Arc;

pub mod id;
pub mod ls;
pub mod upload;
pub mod request;
pub mod peers;
pub mod reify;

#[async_trait]
pub trait WraithCommand {
    async fn execute(&self, node: Arc<SentinelNode>, args: Vec<String>) -> Result<()>;
}

pub fn print_spectral_header(title: &str) {
    println!("\n--- {} ---", title);
}