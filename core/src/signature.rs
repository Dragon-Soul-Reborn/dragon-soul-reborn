//! Verifies signatures made by `tauri signer sign`: the public key and the signature are base64 of the
//! minisign text files. The same key signs launcher updates, server-info and recipes.

use base64::Engine;
use minisign_verify::{PublicKey, Signature};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum SignatureError {
    #[error("the public key is unreadable")]
    BadKey,
    #[error("the signature is unreadable")]
    BadSignature,
    #[error("the signature does not match")]
    Mismatch,
}

pub fn verify(pubkey_b64: &str, data: &[u8], sig_b64: &str) -> Result<(), SignatureError> {
    let key_text = unbase64(pubkey_b64).ok_or(SignatureError::BadKey)?;
    let key = PublicKey::decode(&key_text).map_err(|_| SignatureError::BadKey)?;
    let sig_text = unbase64(sig_b64).ok_or(SignatureError::BadSignature)?;
    let sig = Signature::decode(&sig_text).map_err(|_| SignatureError::BadSignature)?;
    // `true` also accepts non-prehashed signatures, as Tauri's own updater does.
    key.verify(data, &sig, true)
        .map_err(|_| SignatureError::Mismatch)
}

fn unbase64(s: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .ok()?;
    String::from_utf8(bytes).ok()
}
