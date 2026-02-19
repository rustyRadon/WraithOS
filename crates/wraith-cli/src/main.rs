use bip39::{Mnemonic, Language};
use serde::{Serialize, Deserialize};
use rand::RngCore;
use ed25519_dalek::SigningKey;
use std::io::{self, Write};

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
struct WraithIdentity {
    seed_phrase: String,
    public_key: String,
}

fn main() {
    print_banner();

    // Load or Create Identity
    let identity: WraithIdentity = confy::load("wraith-os", "identity").unwrap_or_default();

    let active_identity = if identity.seed_phrase.is_empty() {
        println!("No spectral identity found in the void.");
        println!("Press ENTER to summon a new identity, or type your 24 words to manifest...");
        
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let trimmed = input.trim();

        if trimmed.is_empty() {
            summon_new_identity()
        } else {
            manifest_from_words(trimmed)
        }
    } else {
        println!("Welcome back, Ghost.");
        println!("Node ID: {}", identity.public_key);
        identity
    };

    // Enter the Sentinel Loop
    boot_sentinel(active_identity);
}

fn print_banner() {
    println!("-------------------------------------------");
    println!("   ⛧ W R A I T H   O S :  C L I ⛧         ");
    println!("       [ Sentinel Node v0.1.0 ]            ");
    println!("-------------------------------------------");
}

fn summon_new_identity() -> WraithIdentity {
    let mut entropy = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut entropy);

    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
        .expect("Failed to create mnemonic");
    let phrase = mnemonic.to_string();

    let identity = derive_identity_from_phrase(&phrase);
    
    println!("\n⚠️  NEW RECOVERY PHRASE GENERATED ⚠️");
    println!("-------------------------------------------");
    println!("{}", phrase);
    println!("-------------------------------------------");
    println!("Identity bound to system config.");

    confy::store("wraith-os", "identity", &identity).expect("Failed to save identity");
    identity
}

fn manifest_from_words(phrase: &str) -> WraithIdentity {
    match Mnemonic::parse_in_normalized(Language::English, phrase) {
        Ok(_) => {
            let identity = derive_identity_from_phrase(phrase);
            confy::store("wraith-os", "identity", &identity).expect("Failed to save identity");
            println!("Identity manifested and stored.");
            identity
        },
        Err(e) => {
            println!("Error: The words do not match the ritual requirements: {}", e);
            std::process::exit(1);
        }
    }
}

fn derive_identity_from_phrase(phrase: &str) -> WraithIdentity {
    let mnemonic = Mnemonic::parse_in_normalized(Language::English, phrase).unwrap();
    let seed = mnemonic.to_seed(""); 
    let seed_32: [u8; 32] = seed[0..32].try_into().expect("Seed conversion failed");
    
    let signing_key = SigningKey::from_bytes(&seed_32);
    let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());

    WraithIdentity {
        seed_phrase: phrase.to_string(),
        public_key: public_key_hex,
    }
}

fn boot_sentinel(id: WraithIdentity) {
    println!("Initializing Sentinel Engine for Node {}...", id.public_key);
    
    loop {
        print!("\nwraith@{} >> ", &id.public_key[..8]);
        io::stdout().flush().unwrap();

        let mut cmd = String::new();
        io::stdin().read_line(&mut cmd).unwrap();
        let cmd = cmd.trim();

        match cmd {
            "exit" | "banish" => {
                println!("Closing spectral connection...");
                break;
            },
            "id" => println!("Node ID: {}", id.public_key),
            "peers" => println!("Scanning the void for peers... [0 Found]"),
            "help" => println!("Commands: id, peers, help, banish"),
            _ => println!("Unknown ritual command: {}", cmd),
        }
    }
}