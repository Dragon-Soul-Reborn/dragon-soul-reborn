//! A pinned text-only correction for the original December 2011 Windows client.
//! No original executable is distributed: verified installer bytes are transformed locally.
use crate::hash;
use std::io;

pub const ORIGINAL_SHA256: &str =
    "af1d0fa718f55a17ea8c05632bb92ca71c0ff003daed4e0901a128c546910991";
pub const PATCHED_SHA256: &str = "5774e3bde5761086075a443b07eab89f2749c99156af38bcbf358c891bca58a3";
pub const MESSAGE: &[u8] = b"Incorrect username or password.\0";

pub fn patch(original: &[u8]) -> io::Result<Vec<u8>> {
    if hash::sha256_bytes(original) != ORIGINAL_SHA256 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported client version for login-message correction",
        ));
    }
    let mut bytes = original.to_vec();
    edit(&mut bytes)?;
    if hash::sha256_bytes(&bytes) != PATCHED_SHA256 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "login-message correction produced unexpected bytes",
        ));
    }
    Ok(bytes)
}

fn edit(bytes: &mut [u8]) -> io::Result<()> {
    // Extend .rdata's virtual length into existing zero padding; no file offsets change.
    replace(
        bytes,
        0x240,
        &0x88b62u32.to_le_bytes(),
        &0x88b84u32.to_le_bytes(),
    )?;
    replace(bytes, 0x48bb64, &[0; 32], MESSAGE)?;
    // Login-failure code 4 uses this relocated pointer. Database-error code 3 is untouched.
    replace(
        bytes,
        0x48e388,
        &0x824a84u32.to_le_bytes(),
        &0x88bb64u32.to_le_bytes(),
    )
}

fn replace(bytes: &mut [u8], offset: usize, expected: &[u8], replacement: &[u8]) -> io::Result<()> {
    let target = bytes
        .get_mut(offset..offset + expected.len())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "truncated client"))?;
    if target != expected || expected.len() != replacement.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected client layout",
        ));
    }
    target.copy_from_slice(replacement);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 0x48e390];
        bytes[0x240..0x244].copy_from_slice(&0x88b62u32.to_le_bytes());
        bytes[0x48e384..0x48e388].copy_from_slice(&0x824a74u32.to_le_bytes());
        bytes[0x48e388..0x48e38c].copy_from_slice(&0x824a84u32.to_le_bytes());
        bytes
    }
    #[test]
    fn changes_only_message_storage_and_bad_credentials_pointer() {
        let original = fixture();
        let mut bytes = original.clone();
        edit(&mut bytes).unwrap();
        assert_eq!(bytes.len(), original.len());
        assert_eq!(&bytes[0x48bb64..0x48bb84], MESSAGE);
        assert_eq!(&bytes[0x48e388..0x48e38c], &0x88bb64u32.to_le_bytes());
        assert_eq!(&bytes[0x48e384..0x48e388], &original[0x48e384..0x48e388]);
        for (i, (before, after)) in original.iter().zip(&bytes).enumerate() {
            if before != after {
                assert!(
                    (0x240..0x244).contains(&i)
                        || (0x48bb64..0x48bb84).contains(&i)
                        || (0x48e388..0x48e38c).contains(&i)
                );
            }
        }
    }
    #[test]
    fn rejects_unrecognised_version_even_with_expected_layout() {
        assert!(patch(&fixture()).is_err());
        assert!(patch(b"truncated").is_err());
    }
    #[test]
    fn refuses_occupied_padding() {
        let mut bytes = fixture();
        bytes[0x48bb64] = 1;
        assert!(edit(&mut bytes).is_err());
    }
}
