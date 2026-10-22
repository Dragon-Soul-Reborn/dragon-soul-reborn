use dsr_core::aup;
use std::{fs, path::PathBuf};

const VECTORS: &[&str] = &[
    "empty", "one", "two", "three", "text", "random", "runs", "xml",
];

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/aup")
            .join(name),
    )
    .unwrap()
}

#[test]
fn encode_matches_the_python_codec_byte_for_byte() {
    for v in VECTORS {
        assert_eq!(
            aup::encode(&fixture(&format!("{v}.bin"))),
            fixture(&format!("{v}.aup")),
            "vector {v}"
        );
    }
}

#[test]
fn decode_restores_every_vector() {
    for v in VECTORS {
        let decoded = aup::decode(&fixture(&format!("{v}.aup"))).unwrap();
        assert_eq!(decoded, fixture(&format!("{v}.bin")), "vector {v}");
    }
}

#[test]
fn decode_rejects_a_truncated_header() {
    assert_eq!(aup::decode(&[1, 2]), Err(aup::AupError::TooSmall));
}

#[test]
fn decode_rejects_a_wrong_size_header() {
    let mut blob = aup::encode(b"hello hello hello");
    blob[0] = blob[0].wrapping_add(1);
    assert!(matches!(
        aup::decode(&blob),
        Err(aup::AupError::SizeMismatch { .. })
    ));
}
