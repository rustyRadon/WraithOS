use slint::{SharedString, ModelRc, VecModel, Model};
use std::rc::Rc;
use bip39::{Mnemonic, Language};
use ed25519_dalek::SigningKey;
use serde::{Serialize, Deserialize};
use rand::RngCore;

slint::include_modules!();

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
struct WraithIdentity {
    seed_phrase: String,
    public_key: String,
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;

    // --- SPECTRAL LINK: LOAD EXISTING IDENTITY ON STARTUP ---
    let existing_id: WraithIdentity = confy::load("wraith-os", "identity").unwrap_or_default();
    
    if !existing_id.seed_phrase.is_empty() {
        // 1. Map words to the UI grid
        let words_vec: Vec<SharedString> = existing_id.seed_phrase
            .split_whitespace()
            .map(SharedString::from)
            .collect();
        
        ui.set_words(ModelRc::from(Rc::new(VecModel::from(words_vec))));
        
        // 2. Set the Node ID and show the "Welcome Home" state
        ui.set_node_id(SharedString::from(&existing_id.public_key));
        ui.set_manifesting(true);
    }

    // --- CALLBACK: MANIFEST IDENTITY ---
    ui.on_manifest_identity({
        let ui_handle = ui.as_weak();
        move |_| {
            let ui = ui_handle.unwrap();
            let words_model = ui.get_words();
            let phrase_vec: Vec<String> = words_model.iter().map(|s| s.to_string()).collect();
            let combined_phrase = phrase_vec.join(" ");

            if let Ok(mnemonic) = Mnemonic::parse_in_normalized(Language::English, &combined_phrase) {
                let seed = mnemonic.to_seed(""); 
                let seed_32: [u8; 32] = seed[0..32].try_into().unwrap();
                let signing_key = SigningKey::from_bytes(&seed_32);
                let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());

                ui.set_node_id(SharedString::from(&public_key_hex));
                ui.set_manifesting(true);
                
                let id = WraithIdentity { seed_phrase: combined_phrase, public_key: public_key_hex };
                let _ = confy::store("wraith-os", "identity", id);
            }
        }
    });

    // --- CALLBACK: GENERATE IDENTITY ---
    ui.on_generate_identity({
        let ui_handle = ui.as_weak();
        move || {
            let ui = ui_handle.unwrap();
            let mut entropy = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut entropy);
            let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy).unwrap();
            
            let words_vec: Vec<SharedString> = mnemonic.word_iter()
                .map(SharedString::from)
                .collect();

            ui.set_words(ModelRc::from(Rc::new(VecModel::from(words_vec))));
            ui.set_manifesting(false); // Stay in setup mode so they can see the words
        }
    });

    ui.run()
}