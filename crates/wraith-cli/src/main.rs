use anyhow::Result;
use sentinel_crypto::NodeIdentity;
use std::io::{self, Write};
use std::path::Path;

fn main() -> Result<()> {
    print_banner();

    // Path to the persistent identity file
    let key_path = "identity.key";

    // 1. Load existing or enter setup ritual
    let identity = if Path::new(key_path).exists() {
        println!("Welcome back, Ghost. Identity loaded from sector.");
        NodeIdentity::load_or_generate(key_path)?
    } else {
        perform_initial_ritual(key_path)?
    };

    // 2. Start the interactive Sentinel loop
    boot_sentinel(identity);

    Ok(())
}

fn print_banner() {
    println!("-------------------------------------------");
    println!("   ⛧ W R A I T H   O S :  C L I ⛧         ");
    println!("       [ Sentinel Node v0.1.0 ]            ");
    println!("-------------------------------------------");
}

fn perform_initial_ritual(key_path: &str) -> Result<NodeIdentity> {
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

    // Save the derived key for future sessions
    id.save(key_path)?;
    println!("Identity bound to file: {}", key_path);
    Ok(id)
}

fn boot_sentinel(id: NodeIdentity) {
    println!("Initializing Sentinel Engine for Node {}...", id.node_id());
    
    loop {
        // Use a slice of the Node ID for the prompt
        print!("\nwraith@{} >> ", &id.node_id()[..8]);
        io::stdout().flush().unwrap();

        let mut cmd = String::new();
        io::stdin().read_line(&mut cmd).unwrap();
        let cmd = cmd.trim();

        match cmd {
            "exit" | "banish" => {
                println!("Closing spectral connection...");
                break;
            },
            "id" => {
                println!("Public Node ID: {}", id.node_id());
                println!("Private Key:    {}", id.private_key_hex());
                if let Some(p) = &id.mnemonic_phrase {
                    println!("Recovery Phrase: {}", p);
                }
            },
            "peers" => println!("Scanning the void for peers... [Searching via sentinel-transport]"),
            "help" => println!("Commands: id, peers, help, exit"),
            _ => println!("Unknown ritual command: {}", cmd),
        }
    }
}

fn prompt(msg: &str) -> String {
    print!("{}", msg);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}