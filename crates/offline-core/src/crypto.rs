use aes_gcm::{aead::{Aead, Payload}, Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;
use crate::{Error, Result};

pub fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
pub fn b64(bytes: &[u8]) -> String { STANDARD.encode(bytes) }
pub fn decode(text: &str) -> Result<Vec<u8>> { STANDARD.decode(text).map_err(|_| Error::Signature) }
pub fn sign(key: &SigningKey, domain: &str, payload: &[u8]) -> String {
    let mut bytes=domain.as_bytes().to_vec(); bytes.push(0); bytes.extend(payload);
    b64(&key.sign(&bytes).to_bytes())
}
pub fn verify(key: &[u8;32], domain: &str, payload: &[u8], signature: &str) -> Result<()> {
    let mut bytes=domain.as_bytes().to_vec(); bytes.push(0); bytes.extend(payload);
    let signature=Signature::from_slice(&decode(signature)?).map_err(|_| Error::Signature)?;
    VerifyingKey::from_bytes(key).map_err(|_| Error::Signature)?
        .verify_strict(&bytes,&signature).map_err(|_| Error::Signature)
}
pub struct DataKey(Zeroizing<[u8;32]>);
impl DataKey {
    pub fn new(bytes: [u8;32]) -> Self { Self(Zeroizing::new(bytes)) }
    pub(crate) fn export(&self)->[u8;32] { *self.0 }
    pub fn seal(&self, aad: &str, bytes: &[u8]) -> Result<Vec<u8>> {
        let cipher=Aes256Gcm::new_from_slice(self.0.as_ref()).map_err(|_| Error::Encryption)?;
        let mut nonce=[0u8;12]; OsRng.fill_bytes(&mut nonce);
        let encrypted=cipher.encrypt(Nonce::from_slice(&nonce),Payload{msg:bytes,aad:aad.as_bytes()})
            .map_err(|_|Error::Encryption)?;
        let mut output=vec![1]; output.extend(nonce); output.extend(encrypted); Ok(output)
    }
    pub fn open(&self, aad: &str, bytes: &[u8]) -> Result<Vec<u8>> {
        if bytes.len()<29 || bytes[0]!=1 { return Err(Error::Encryption); }
        Aes256Gcm::new_from_slice(self.0.as_ref()).map_err(|_|Error::Encryption)?
            .decrypt(Nonce::from_slice(&bytes[1..13]),Payload{msg:&bytes[13..],aad:aad.as_bytes()})
            .map_err(|_|Error::Encryption)
    }
}
