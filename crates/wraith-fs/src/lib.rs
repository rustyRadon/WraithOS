use anyhow::{anyhow, Result};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use std::fs::File;
use std::io::Read;
use uuid::Uuid;

const CHUNK_SIZE: usize = 1024 * 1024;

pub fn slice_it_up(path: &str, key: &[u8; 32]) -> Result<(Uuid, Vec<Vec<u8>>)> {
    let mut f = match File::open(path) {
        Ok(file) => file,
        Err(e) => return Err(anyhow!("couldn't open file {}: {}", path, e)),
    };
    
    let file_id = Uuid::new_v4();
    let mut chunks = Vec::new();
    
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
                let mut nonce_bytes = [0u8; 12];
                nonce_bytes[..4].copy_from_slice(&idx.to_le_bytes());
                let nonce = Nonce::from_slice(&nonce_bytes);

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

pub fn decrypt_chunk(encrypted_data: &[u8], key: &[u8; 32], chunk_index: u32) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| anyhow!("invalid key length"))?;

    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[..4].copy_from_slice(&chunk_index.to_le_bytes());
    let nonce = Nonce::from_slice(&nonce_bytes);

    let decrypted = cipher.decrypt(nonce, encrypted_data)
        .map_err(|_| anyhow!("failed to decrypt chunk"))?;

    Ok(decrypted)
}