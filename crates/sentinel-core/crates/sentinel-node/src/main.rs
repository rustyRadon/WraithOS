use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

// Imports from your clean library
use sentinel_core::{SentinelNode, discovery, SentinelEvent};
use sentinel_protocol::messages::{MessageContent, SentinelMessage};

mod handlers;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "./.sentinel")]
    data_dir: PathBuf,
    #[arg(short, long, default_value_t = 8443)]
    port: u16,
    #[arg(short, long, default_value = "127.0.0.1:8888")]
    signaler: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let args = Args::parse();

    // 1. Initialize Engine
    let (node_struct, signaler_rx) = SentinelNode::new(args.data_dir, args.port).await?;
    let node = Arc::new(node_struct);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    // 2. Start Discovery & Engine (The Engine now owns the TcpListener!)
    discovery::start_discovery(Arc::clone(&node), args.port).await?;
    
    let engine_node = Arc::clone(&node);
    tokio::spawn(engine_node.run(event_tx));

    // 3. Start Background Services
    let sig_node = Arc::clone(&node);
    let sig_addr = args.signaler.clone();
    tokio::spawn(async move { sig_node.start_signaler_client(sig_addr, signaler_rx).await; });
    tokio::spawn(Arc::clone(&node).start_gossip_service());
    tokio::spawn(Arc::clone(&node).start_heartbeat_service());

    // 4. Event UI Loop (Prints messages from the Engine)
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                SentinelEvent::ChatMessage { sender, text } => {
                    println!("\n[{}] {}", sender, text);
                }
                SentinelEvent::SystemLog(msg) => {
                    println!("[SYSTEM] {}", msg);
                }
                SentinelEvent::PeerConnected { peer_id, .. } => {
                    println!("[+] Connected to: {}", peer_id);
                }
                _ => {}
            }
        }
    });

    println!("SENTINEL ACTIVE. ID: {}", node.identity.node_id());
    println!("SYSTEM READY. Input commands below.");

    // 5. Input & Shutdown Logic
    tokio::select! {
        res = handlers::handle_stdin(Arc::clone(&node)) => {
            if let Err(e) = res { eprintln!("Terminal error: {}", e); }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\n[!] Shutdown signal received.");
        }
    }

    // Graceful Shutdown
    let goodbye = SentinelMessage::new(
        node.identity.node_id(), 
        MessageContent::Disconnect("Node shutting down".into())
    );
    for entry in node.peers.iter() {
        let _ = entry.value().tx.send(goodbye.clone());
    }
    let _ = node.db.flush_async().await;
    
    Ok(())
}