use bip39::{Mnemonic, Language};
use serde::{Serialize, Deserialize};
use rand::RngCore;
//use sentinel_crypto::NodeIdentity as _;
use ed25519_dalek::SigningKey;

#[derive(Serialize, Deserialize, Debug, Default)]
struct WraithIdentity {
    seed_phrase: String,
    public_key: String,
}

fn main() {
    println!("--- WraithOS: Spectral Initialization ---");

    let identity: WraithIdentity = confy::load("wraith-os", "identity").unwrap_or_default();

    if identity.seed_phrase.is_empty() {
        println!("No spectral identity found. Summoning new keys...");
        summon_new_identity();
    } else {
        println!("Welcome back, Ghost.");
        println!("Node ID: {}", identity.public_key);
        boot_sentinel(identity);
    }
}

fn summon_new_identity() {
    let mut entropy = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut entropy);

    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
        .expect("Failed to create mnemonic");
    let phrase = mnemonic.to_string();

    let seed = mnemonic.to_seed(""); 
    let seed_32: [u8; 32] = seed[0..32].try_into().expect("Seed conversion failed");
    
    // wrap your bih ahh up in NodeIdentity struct
    let signing_key = SigningKey::from_bytes(&seed_32);
    
    let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());

    println!("\n⚠️  RECOVERY PHRASE (SAVE THIS!) ⚠️");
    println!("-------------------------------------------");
    println!("{}", phrase);
    println!("-------------------------------------------");

    let new_id = WraithIdentity {
        seed_phrase: phrase,
        public_key: public_key_hex,
    };

    confy::store("wraith-os", "identity", new_id).expect("Failed to save identity");
    println!("Identity bound to system.");
}

fn boot_sentinel(_id: WraithIdentity) {
    println!("Starting Sentinel Engine...");
    // logic for P2P connection 
}


