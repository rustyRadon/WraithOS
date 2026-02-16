use slint::{SharedString, ModelRc, VecModel, Timer, TimerMode}; 
use std::rc::Rc;
use rand::RngCore;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let ui_handle = ui.as_weak();

    let timer = Timer::default();
    let timer_handle = ui.as_weak();
    
    timer.start(TimerMode::Repeated, std::time::Duration::from_millis(30), move || {
        if let Some(ui) = timer_handle.upgrade() {
            if ui.get_manifesting() {
                ui.invoke_update_flicker();
            }
        }
    });

    ui.on_generate_identity({
        let ui_handle = ui.as_weak();
        move || {
            let ui = ui_handle.unwrap();
            let is_manifesting = ui.get_manifesting();
            
            if !is_manifesting {
                let mut entropy = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut entropy);
                let mnemonic = bip39::Mnemonic::from_entropy_in(bip39::Language::English, &entropy).unwrap();
                let words_vec: Vec<SharedString> = mnemonic.words().map(SharedString::from).collect();
                ui.set_words(ModelRc::from(Rc::new(VecModel::from(words_vec))));
                ui.set_node_id(SharedString::from("GHOST-REAPER-666"));
            }
        }
    });

    ui.run()
}