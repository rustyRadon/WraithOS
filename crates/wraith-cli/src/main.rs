use anyhow::{Context, Result};
use sentinel_core::engine::SentinelNode;
use sentinel_crypto::NodeIdentity;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// Using the full feature set of tokio to allow async main
#[tokio::main]
async fn main() -> Result<()> {
    print_banner();

    // Setup data directory for persistence
    let data_dir = PathBuf::from("."); 
    let key_path = data_dir.join("identity.key");

    // 1. Load or Summon Identity
    let identity = if key_path.exists() {
        println!("Welcome back, Ghost. Identity loaded from sector.");
        NodeIdentity::load_or_generate(key_path.to_str().unwrap())?
    } else {
        perform_initial_ritual(&key_path)?
    };

    // 2. Initialize the Sentinel Engine (The Brain)
    // Explicitly destructure to help the compiler with types
    let (node_raw, _signaler_rx) = SentinelNode::new(data_dir, 5000).await
        .context("Failed to initialize Sentinel Engine")?;
    let node = Arc::new(node_raw);

    // 3. Spawn Engine Background Tasks
    // FIX: Explicitly define the channel type to resolve E0282
    let (event_tx, mut _event_rx) = tokio::sync::mpsc::unbounded_channel::<sentinel_core::SentinelEvent>();
    
    let node_inner = Arc::clone(&node);
    tokio::spawn(async move {
        if let Err(e) = node_inner.run(event_tx).await {
            eprintln!("Engine runtime error: {}", e);
        }
    });

    // Start background services: Heartbeat, Discovery, etc.
    tokio::spawn(Arc::clone(&node).start_heartbeat_service());
    
    // Ensure discovery starts - node and port
    let discovery_node = Arc::clone(&node);
    tokio::spawn(async move {
        let _ = sentinel_core::discovery::start_discovery(discovery_node, 5000).await;
    });

    // 4. Enter the Interactive Sentinel Loop
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
            },
            "upload" | "manifest" => {
                if parts.len() < 2 {
                    println!("Usage: upload <file_path>");
                    continue;
                }
                let path = parts[1];
                
                println!("Slicing file into the void...");
                
                // Derive a 32-byte key from the identity for encryption
                let mut key = [0u8; 32];
                let pk_bytes = node.identity.public_key_bytes();
                let len = pk_bytes.len().min(32);
                key[..len].copy_from_slice(&pk_bytes[..len]);

                // Call the slicer logic from wraith-fs
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
                        if files.is_empty() {
                            println!("[Empty Void]");
                        }
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
                    println!("[Scanning...] No peers found in current sector.");
                }
                for entry in node.peers.iter() {
                    println!("Peer: {} | Name: {}", entry.key(), entry.value().node_name);
                }
            },
            "help" => {
                println!("Commands:");
                println!("  id       - Show node identity details");
                println!("  upload   - Slice and encrypt a file into storage");
                println!("  ls       - List stored documents");
                println!("  peers    - List connected sentinel nodes");
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