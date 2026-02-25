use anyhow::{Context, Result};
use sentinel_core::engine::SentinelNode;
use sentinel_crypto::NodeIdentity;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    // 0. Install Crypto Provider (Required for Rustls 0.23+)
    let _ = rustls::crypto::ring::default_provider().install_default();

    // 1. Parse CLI Arguments: cargo run -- <port> <data_dir>
    let args: Vec<String> = std::env::args().collect();
    let listen_port: u16 = args.get(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(5000); // Default to 5000 if not specified
    
    let data_path = args.get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".")); // Default to current dir

    print_banner();

    // Ensure the data directory exists
    if !data_path.exists() {
        std::fs::create_dir_all(&data_path)?;
    }

    let key_path = data_path.join("identity.key");

    // 2. Load or Summon Identity
    let _identity = if key_path.exists() {
        println!("Welcome back, Ghost. Identity loaded from sector: {}", data_path.display());
        NodeIdentity::load_or_generate(key_path.to_str().unwrap())?
    } else {
        perform_initial_ritual(&key_path)?
    };

    // 3. Initialize the Sentinel Engine
    // Note: SentinelNode::new now returns the node and the signaler_rx channel
    let (node_raw, signaler_rx) = SentinelNode::new(data_path, listen_port).await
        .context("Failed to initialize Sentinel Engine")?;
    let node = Arc::new(node_raw);

    // 4. Spawn Engine Background Tasks
    let (event_tx, mut _event_rx) = mpsc::unbounded_channel::<sentinel_core::SentinelEvent>();
    
    // TASK: Main Engine Runner (Incoming TCP/TLS)
    let engine_node = Arc::clone(&node);
    tokio::spawn(async move {
        let _ = engine_node.run(event_tx).await;
    });

    // TASK: Heartbeat Service (Keep-alive PINGs)
    tokio::spawn(Arc::clone(&node).start_heartbeat_service());
    
    // TASK: mDNS Discovery (Local WiFi whispers)
    let discovery_node = Arc::clone(&node);
    tokio::spawn(async move {
        let _ = sentinel_core::discovery::start_discovery(discovery_node, listen_port).await;
    });

    // TASK: STUN Discovery (Find our Public IP for the Signaler)
    let stun_node = Arc::clone(&node);
    tokio::spawn(async move {
        if let Err(e) = stun_node.discover_and_set_public_ip().await {
            eprintln!("[!] STUN Warning: Could not resolve public IP: {}", e);
        }
    });

    // TASK: Signaler Client (Connect to the Global Lighthouse)
    let signaler_node = Arc::clone(&node);
    let signaler_addr = "127.0.0.1:8888".to_string(); // Update to your VPS IP for global use
    tokio::spawn(async move {
        signaler_node.start_signaler_client(signaler_addr, signaler_rx).await;
    });

    // 5. Enter the Interactive Sentinel Loop
    boot_sentinel(node).await?;

    Ok(())
}

fn print_banner() {
    println!("-------------------------------------------");
    println!("   ⛧ W R A I T H   O S :  C L I ⛧         ");
    println!("       [ Sentinel Node v0.1.0 ]            ");
    println!("-------------------------------------------");
}

async fn boot_sentinel(node: Arc<SentinelNode>) -> Result<()> {
    println!("Initializing Sentinel Engine for Node {}...", node.identity.node_id());
    
    loop {
        print!("\nwraith@{} >> ", &node.identity.node_id()[..8]);
        io::stdout().flush().unwrap();

        let mut cmd_line = String::new();
        io::stdin().read_line(&mut cmd_line)?;
        let parts: Vec<&str> = cmd_line.trim().split_whitespace().collect();
        
        if parts.is_empty() { continue; }

        match parts[0] {
            "exit" | "banish" => {
                println!("Closing spectral connection...");
                break;
            },
            "id" => {
                println!("Public Node ID: {}", node.identity.node_id());
                if let Some(p) = &node.identity.mnemonic_phrase {
                    println!("Recovery Phrase: {}", p);
                }
                if let Some(ip) = *node.public_addr.read().await {
                    println!("Public IP: {}", ip);
                }
            },
            "dial" | "connect" => {
                if parts.len() < 2 {
                    println!("Usage: dial <ip>:<port>");
                    continue;
                }
                let target_addr = parts[1].to_string();
                println!("Attempting to pierce the veil to {}...", target_addr);

                let node_clone = Arc::clone(&node);
                tokio::spawn(async move {
                    match node_clone.dial_peer(target_addr.clone()).await {
                        Ok(_) => println!("\n[+] Connection established with {}", target_addr),
                        Err(e) => println!("\n[-] Failed to reach {}: {}", target_addr, e),
                    }
                });
            },
            "upload" | "manifest" => {
                if parts.len() < 2 {
                    println!("Usage: upload <file_path>");
                    continue;
                }
                let path = parts[1];
                println!("Slicing file into the void...");
                
                let mut key = [0u8; 32];
                let pk_bytes = node.identity.public_key_bytes();
                let len = pk_bytes.len().min(32);
                key[..len].copy_from_slice(&pk_bytes[..len]);

                match wraith_fs::slice_it_up(path, &key) {
                    Ok((file_id, chunks)) => {
                        node.ingest_file(path, file_id, chunks)?;
                        println!("✓ File Manifested. ID: {}", file_id);
                    }
                    Err(e) => println!("✗ Failed to manifest file: {}", e),
                }
            },
            "ls" | "library" => {
                match node.get_local_library() {
                    Ok(files) => {
                        println!("--- Local Spectral Library ---");
                        if files.is_empty() { println!("[Empty Void]"); }
                        for f in files {
                            println!("{}: {} ({} chunks)", f.id, f.name, f.total_chunks);
                        }
                    }
                    Err(e) => println!("Error reading library: {}", e),
                }
            },
            "peers" => {
                println!("--- Active Peer Handshakes ---");
                if node.peers.is_empty() {
                    println!("[Scanning...] No active peer connections.");
                }
                for entry in node.peers.iter() {
                    let peer = entry.value();
                    println!("Peer ID: {} | Name: {} | Status: Active", 
                        peer.node_id, 
                        peer.node_name
                    );
                }
            },
            "scan" | "refresh" => {
                if parts.len() < 2 {
                    println!("Usage: scan <peer_addr_or_id>");
                    continue;
                }
                let target = parts[1].to_string();
                println!("Requesting library from {}...", target);
                
                let node_clone = Arc::clone(&node);
                tokio::spawn(async move {
                    if let Err(e) = node_clone.request_peer_library(&target).await {
                        println!("[-] Scan failed: {}", e);
                    }
                });
            },
            "request" | "friend" => {
                if parts.len() < 2 {
                    println!("Usage: request <public_key>");
                    continue;
                }
                
                let target_id = parts[1].to_string();
                let node_clone = Arc::clone(&node);

                println!("Initiating spectral search for: {}...", target_id);

                tokio::spawn(async move {
                    match node_clone.request_ghost_by_id(target_id.clone()).await {
                        Ok(_) => println!("\n[✓] Connection established with Ghost {}", &target_id[..8]),
                        Err(e) => println!("\n[-] Discovery failed for {}: {}", &target_id[..8], e),
                    }
                });
            },
            "help" => {
                println!("Commands:");
                println!("  id       - Show your local Node ID and Public IP");
                println!("  dial     - Direct connect to an IP:Port");
                println!("  request  - Find a Ghost by Public Key (Global/Local)");
                println!("  peers    - List active connections");
                println!("  upload   - Manifest a local file");
                println!("  ls       - List your files");
                println!("  scan     - Get a peer's file library");
                println!("  exit     - Shutdown node");
            },
            _ => println!("Unknown ritual command: {}. Type 'help' for guidance.", parts[0]),
        }
    }
    Ok(())
}

fn perform_initial_ritual(key_path: &Path) -> Result<NodeIdentity> {
    println!("No spectral identity found in the void.");
    println!("[1] Manifest (Input existing Private Key or 24 Words)");
    println!("[2] Summon (Generate a new 24-word identity)");

    let choice = prompt("Selection: ");

    let id = match choice.as_str() {
        "1" => {
            println!("Choose your medium: [K]ey (Hex) or [W]ords (24 words)");
            let medium = prompt("> ").to_lowercase();
            if medium == "k" {
                let hex = prompt("Enter Private Key (Hex): ");
                NodeIdentity::from_hex_key(&hex)?
            } else {
                let words = prompt("Enter 24 words: ");
                NodeIdentity::from_mnemonic(&words)?
            }
        }
        "2" => {
            let new_id = NodeIdentity::generate_with_mnemonic();
            println!("\n⚠️  NEW RECOVERY PHRASE GENERATED ⚠️");
            println!("-------------------------------------------");
            println!("{}", new_id.mnemonic_phrase.as_ref().unwrap());
            println!("-------------------------------------------");
            println!("Record these words. They are your only bridge to the void.");
            new_id
        }
        _ => return Err(anyhow::anyhow!("Invalid ritual selection.")),
    };

    id.save(key_path.to_str().unwrap())?;
    println!("Identity bound to file: {}", key_path.display());
    Ok(id)
}

fn prompt(msg: &str) -> String {
    print!("{}", msg);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}