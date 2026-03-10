use anyhow::{Context, Result};
use sentinel_core::engine::SentinelNode;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

mod commands;
mod ritual;

use commands::{
    WraithCommand, 
    id::IdCommand, 
    ls::LsCommand, 
    upload::UploadCommand,
    peers::PeersCommand,
    request::RequestCommand
};

#[tokio::main]
async fn main() -> Result<()> {
    // 0. Global Security Provider initialization
    let _ = rustls::crypto::ring::default_provider().install_default();

    // 1. mi setup
    let args: Vec<String> = std::env::args().collect();
    let port: u16 = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(5000);
    let data_path = args.get(2).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));

    print_banner();

    if !data_path.exists() { 
        std::fs::create_dir_all(&data_path)?; 
    }
    let key_path = data_path.join("identity.key");

    // 2. Identity Ritual heheheheheh
    let _identity = if key_path.exists() {
        println!("Welcome back, Ghost. Sector: {}", data_path.display());
        sentinel_crypto::NodeIdentity::load_or_generate(key_path.to_str().unwrap())?
    } else {
        ritual::perform_initial_ritual(&key_path)?
    };

    // 3. start the mentinel engine vooom vommm
    let (node_raw, signaler_rx) = SentinelNode::new(data_path, port).await
        .context("Failed to ignite Sentinel Engine")?;
    let node = Arc::new(node_raw);

    let (event_tx, mut _event_rx) = mpsc::unbounded_channel::<sentinel_core::SentinelEvent>();
    
    let core_node = Arc::clone(&node);
    tokio::spawn(async move {
        let _ = core_node.run(event_tx).await;
    });

    tokio::spawn(Arc::clone(&node).start_heartbeat_service());
    
    let discovery_node = Arc::clone(&node);
    tokio::spawn(async move {
        let _ = sentinel_core::discovery::start_discovery(discovery_node, port).await;
    });

    let stun_node = Arc::clone(&node);
    tokio::spawn(async move {
        if let Err(e) = stun_node.discover_and_set_public_ip().await {
            eprintln!("[!] Spectral Alert: STUN failed to resolve external IP: {}", e);
        }
    });

    let signaler_node = Arc::clone(&node);
    let signaler_addr = "127.0.0.1:8888".to_string(); 
    tokio::spawn(async move {
        signaler_node.start_signaler_client(signaler_addr, signaler_rx).await;
    });

    boot_sentinel(node).await?;

    Ok(())
}

async fn boot_sentinel(node: Arc<SentinelNode>) -> Result<()> {
    println!("Sentinel Online: {}", node.identity.node_id());

    loop {
        print!("\nwraith@{} >> ", &node.identity.node_id()[..8]);
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let parts: Vec<String> = input.trim().split_whitespace().map(|s| s.to_string()).collect();
        
        if parts.is_empty() { continue; }

        let cmd_name = &parts[0];
        let args = parts[1..].to_vec();

        match cmd_name.as_str() {
            "exit" | "banish" => {
                println!("⛧ Vanishing from the network...");
                break;
            },
            "id"      => IdCommand.execute(Arc::clone(&node), args).await?,
            "ls"      => LsCommand.execute(Arc::clone(&node), args).await?,
            "upload"  => UploadCommand.execute(Arc::clone(&node), args).await?,
            "peers"   => PeersCommand.execute(Arc::clone(&node), args).await?,
            "request" => RequestCommand.execute(Arc::clone(&node), args).await?,
            "reify" => commands::reify::ReifyCommand.execute(Arc::clone(&node), args).await?,
            "help"    => {
                println!("--- Available Rituals ---");
                println!("  id       - Show local Node ID and Public IP");
                println!("  ls       - List your manifested spectral files");
                println!("  upload   - Slice a local file into the void");
                println!("  peers    - List ghosts currently entangled");
                println!("  request  - Find a ghost by ID via the Lighthouse");
                println!("  exit     - Shutdown the Sentinel engine");
            },
            _ => println!("Unknown ritual: {}. Type 'help' for guidance.", cmd_name),
        }
    }
    Ok(())
}

fn print_banner() {
    println!("-------------------------------------------");
    println!("   ⛧ W R A I T H   O S :  C L I ⛧         ");
    println!("       [ Sentinel Node v0.1.0 ]            ");
    println!("-------------------------------------------");
}