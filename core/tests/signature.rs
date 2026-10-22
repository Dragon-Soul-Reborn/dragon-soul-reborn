mod support;

use dsr_core::signature::{verify, SignatureError};
use std::{fs, path::PathBuf};
use support::sign::TestKey;

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sig")
            .join(name),
    )
    .unwrap()
}

fn text(name: &str) -> String {
    String::from_utf8(fixture(name)).unwrap()
}

#[test]
fn accepts_a_file_signed_by_the_tauri_signer() {
    verify(
        &text("tauri.pub"),
        &fixture("data.txt"),
        &text("data.txt.sig"),
    )
    .unwrap();
}

#[test]
fn rejects_tampered_data() {
    let mut data = fixture("data.txt");
    data[0] ^= 1;
    assert_eq!(
        verify(&text("tauri.pub"), &data, &text("data.txt.sig")),
        Err(SignatureError::Mismatch)
    );
}

#[test]
fn rejects_a_signature_from_another_key() {
    let other = TestKey::new();
    assert_eq!(
        verify(
            &other.public_b64(),
            &fixture("data.txt"),
            &text("data.txt.sig")
        ),
        Err(SignatureError::Mismatch)
    );
}

#[test]
fn rejects_garbage() {
    assert_eq!(
        verify(&text("tauri.pub"), b"x", "not base64!"),
        Err(SignatureError::BadSignature)
    );
    assert_eq!(
        verify("not base64!", b"x", &text("data.txt.sig")),
        Err(SignatureError::BadKey)
    );
}

#[test]
fn accepts_data_signed_at_runtime() {
    let key = TestKey::new();
    verify(&key.public_b64(), b"hello", &key.sign_b64(b"hello")).unwrap();
}
