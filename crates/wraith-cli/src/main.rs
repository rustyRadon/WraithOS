use bip39::{Mnemonic, Language};
use serde::{Serialize, Deserialize};
use rand::RngCore;
// Correct import based on your lib.rs
//use sentinel_crypto::NodeIdentity as _;
// We need this to handle the conversion from seed to signing key
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
    // 1. Generate 32 bytes of random entropy
    let mut entropy = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut entropy);

    // 2. Create mnemonic from that entropy
    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
        .expect("Failed to create mnemonic");
    let phrase = mnemonic.to_string();

    // 3. Turn words into a Seed
    let seed = mnemonic.to_seed(""); 
    let seed_32: [u8; 32] = seed[0..32].try_into().expect("Seed conversion failed");
    
    // --- THE FIX ---
    // Instead of just using SigningKey, we wrap it in your NodeIdentity struct
    let signing_key = SigningKey::from_bytes(&seed_32);
    
    // We create the identity wrapper you defined in sentinel-crypto
    // Note: Since signing_key is private in your struct, we just need to ensure 
    // we use a method that returns the Node ID string.
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
    // Logic for P2P connection goes here next
}

//work more coffee favorite ankle grant meat island plastic despair hockey nominee build tonight remain orange slab hotel snack unable lyrics acoustic coyote mask

