use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use std::fs;
use std::path::Path;
use zeroize::Zeroize;
use bip39::{Mnemonic, Language};

#[derive(Debug)]
pub struct NodeIdentity {
    signing_key: SigningKey,
    pub mnemonic_phrase: Option<String>,
}

impl Drop for NodeIdentity {
    fn drop(&mut self) {
        let mut key_bytes = self.signing_key.to_bytes();
        key_bytes.zeroize();
    }
}

impl NodeIdentity {
    fn from_seed(seed: [u8; 32], phrase: Option<String>) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(&seed),
            mnemonic_phrase: phrase,
        }
    }

    pub fn generate_with_mnemonic() -> Self {
        let mut entropy = [0u8; 32];
        OsRng.fill_bytes(&mut entropy);
        
        let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
            .expect("Failed to generate entropy for mnemonic");
        let phrase = mnemonic.to_string();
        let seed = mnemonic.to_seed("");
        
        let mut seed_bytes: [u8; 32] = seed[0..32].try_into().unwrap();
        let id = Self::from_seed(seed_bytes, Some(phrase));
        
        seed_bytes.zeroize();
        id
    }

    pub fn from_mnemonic(phrase: &str) -> Result<Self> {
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, phrase.trim())
            .map_err(|e| anyhow::anyhow!("Mnemonic parsing failed: {}", e))?;
        let seed = mnemonic.to_seed("");
        let seed_bytes: [u8; 32] = seed[0..32].try_into().unwrap();
        
        Ok(Self::from_seed(seed_bytes, Some(phrase.to_string())))
    }

    pub fn from_hex_key(hex_str: &str) -> Result<Self> {
        let bytes = hex::decode(hex_str.trim())
            .map_err(|e| anyhow::anyhow!("Invalid hex encoding: {}", e))?;
        let array: [u8; 32] = bytes.try_into()
            .map_err(|_| anyhow::anyhow!("Invalid key length: Expected 32 bytes"))?;
        
        Ok(Self::from_seed(array, None))
    }

    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key, mnemonic_phrase: None }
    }

    pub fn load_or_generate<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let exists = path.exists() && fs::metadata(path)?.len() > 0;

        if exists {
            let bytes = fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
            let array: [u8; 32] = bytes.try_into().map_err(|_| anyhow::anyhow!("Invalid key length in file"))?;
            let signing_key = SigningKey::from_bytes(&array);
            Ok(Self { signing_key, mnemonic_phrase: None })
        } else {
            let new_identity = Self::generate();
            new_identity.save(path)?;
            Ok(new_identity)
        }
    }

    pub fn node_id(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_bytes())
    }

    pub fn private_key_hex(&self) -> String {
        hex::encode(self.signing_key.to_bytes())
    }

    pub fn public_key_bytes(&self) -> Vec<u8> {
        self.signing_key.verifying_key().to_bytes().to_vec()
    }

    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.signing_key.sign(message).to_bytes().to_vec()
    }

    pub fn verify(message: &[u8], signature_bytes: &[u8], pubkey_bytes: &[u8]) -> bool {
        if let (Ok(sig), Ok(pubkey)) = (
            Signature::from_slice(signature_bytes),
            VerifyingKey::from_bytes(pubkey_bytes.try_into().unwrap_or(&[0u8; 32])),
        ) {
            return pubkey.verify(message, &sig).is_ok();
        }
        false
    }

    pub fn verify_internal(&self, message: &[u8], signature_bytes: &[u8]) -> bool {
        Self::verify(message, signature_bytes, &self.public_key_bytes())
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        fs::write(path, self.signing_key.to_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_mnemonic_derivation() {
        let id1 = NodeIdentity::generate_with_mnemonic();
        let phrase = id1.mnemonic_phrase.as_ref().unwrap();
        let id2 = NodeIdentity::from_mnemonic(phrase).unwrap();
        assert_eq!(id1.node_id(), id2.node_id());
    }

    #[test]
    fn test_identity_persistence() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        let id1 = NodeIdentity::load_or_generate(path).unwrap();
        let sig = id1.sign(b"wraith-os-test");
        let id2 = NodeIdentity::load_or_generate(path).unwrap();
        assert!(id2.verify_internal(b"wraith-os-test", &sig));
    }
}

