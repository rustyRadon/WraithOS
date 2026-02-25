use anyhow::{anyhow, Result};
// FIX: We must import the Aead trait to use .encrypt()
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use std::fs::File;
// Cleaned up unused Seek/SeekFrom
use std::io::Read;
use uuid::Uuid;

// 1MB chunks - seems to work well in practice
const CHUNK_SIZE: usize = 1024 * 1024;

/// Takes a file and chops it into encrypted pieces
pub fn slice_it_up(path: &str, key: &[u8; 32]) -> Result<(Uuid, Vec<Vec<u8>>)> {
    let mut f = match File::open(path) {
        Ok(file) => file,
        Err(e) => return Err(anyhow!("couldn't open file {}: {}", path, e)),
    };
    
    let file_id = Uuid::new_v4();
    let mut chunks = Vec::new();
    
    // KeyInit provides new_from_slice
    let cipher = match ChaCha20Poly1305::new_from_slice(key) {
        Ok(c) => c,
        Err(_) => return Err(anyhow!("invalid key length")),
    };

    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut idx = 0u32;

    loop {
        match f.read(&mut buf) {
            Ok(0) => break, // EOF, we're done
            Ok(n) => {
                // Use chunk index as nonce - simple but works for this level
                let mut nonce_bytes = [0u8; 12];
                nonce_bytes[..4].copy_from_slice(&idx.to_le_bytes());
                let nonce = Nonce::from_slice(&nonce_bytes);

                // Now that Aead is in scope, .encrypt() will work
                let encrypted = match cipher.encrypt(nonce, &buf[..n]) {
                    Ok(data) => data,
                    Err(_) => return Err(anyhow!("failed to encrypt chunk {}", idx)),
                };
                
                chunks.push(encrypted);
                idx += 1;
            }
            Err(e) => return Err(anyhow!("error reading at chunk {}: {}", idx, e)),
        }
    }

    Ok((file_id, chunks))
}