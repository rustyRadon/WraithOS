use slint::{SharedString, ModelRc, VecModel}; 
use std::rc::Rc;
use rand::RngCore;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;

    // --- THE SUMMONING LOGIC ---
    ui.on_generate_identity({
        let ui_handle = ui.as_weak();
        move || {
            let ui = ui_handle.unwrap();
            
            // Only generate if we are currently dormant
            if !ui.get_manifesting() {
                let mut entropy = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut entropy);
                let mnemonic = bip39::Mnemonic::from_entropy_in(bip39::Language::English, &entropy).unwrap();
                
                let words_vec: Vec<SharedString> = mnemonic.words()
                    .map(SharedString::from)
                    .collect();
                
                ui.set_words(ModelRc::from(Rc::new(VecModel::from(words_vec))));
                ui.set_node_id(SharedString::from("GHOST-REAPER-666"));
            }
            
            // The manifesting toggle remains here
            // This triggers the Slint "animate" blocks and the "root.manifesting" logic
        }
    });

    ui.on_copy_node_id({
        let ui_handle = ui.as_weak();
        move || {
            let ui = ui_handle.unwrap();
            // Optional: Implement actual clipboard logic here later
            println!("Copied node ID: {}", ui.get_node_id());
        }
    });

    ui.run()
}