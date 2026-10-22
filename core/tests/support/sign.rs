//! Throwaway minisign keys that sign like `tauri signer` (base64 of the minisign text files).

use base64::Engine;

pub struct TestKey {
    pk: minisign::PublicKey,
    sk: minisign::SecretKey,
}

impl TestKey {
    pub fn new() -> TestKey {
        let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        TestKey {
            pk: kp.pk,
            sk: kp.sk,
        }
    }

    pub fn public_b64(&self) -> String {
        b64(self.pk.to_box().unwrap().to_string())
    }

    pub fn sign_b64(&self, data: &[u8]) -> String {
        let sig = minisign::sign(
            Some(&self.pk),
            &self.sk,
            std::io::Cursor::new(data),
            None,
            None,
        )
        .unwrap();
        b64(sig.to_string())
    }
}

fn b64(text: String) -> String {
    base64::engine::general_purpose::STANDARD.encode(text)
}
