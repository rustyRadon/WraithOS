use anyhow::{Result, anyhow};
use sentinel_crypto::NodeIdentity;
use std::io::{self, Write};
use std::path::Path;

pub fn perform_initial_ritual(key_path: &Path) -> Result<NodeIdentity> {
    println!("\n⛧ No spectral identity found in the void.");
    println!("[1] Manifest (Input existing Private Key or 24 Words)");
    println!("[2] Summon   (Generate a new 24-word identity)");

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
        _ => return Err(anyhow!("Invalid ritual selection.")),
    };

    id.save(key_path.to_str().unwrap())?;
    println!("✓ Identity bound to file: {}", key_path.display());
    Ok(id)
}

fn prompt(msg: &str) -> String {
    print!("{}", msg);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}