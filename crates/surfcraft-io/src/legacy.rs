//! Legacy encrypted `.brd` files (`docs/spec/file-formats.md` §4): a 12-byte `%BRD-1.0x` header followed by a
//! plain `.brd` file encrypted with PKCS #5 v1.5 "PBEWithMD5AndDES" under a fixed salt and a password that
//! depends on the version. They are only read, never written.

use crate::Error;

/// Length of the `%BRD-1.0x␠␠␊` header.
const HEADER_LEN: usize = 12;

/// Whether the bytes start with the header of a legacy encrypted `.brd` file.
pub fn is_encrypted(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%BRD-")
}

/// Password of a header version, `None` for versions this reader does not know.
fn password(bytes: &[u8]) -> Option<&'static [u8]> {
    if bytes.starts_with(b"%BRD-1.02") {
        Some(b"deltaXTaildeltaXMiddle")
    } else if bytes.starts_with(b"%BRD-1.01") {
        Some(b"deltaXTail")
    } else {
        None
    }
}

/// The header's first line, for error messages.
fn header_line(bytes: &[u8]) -> String {
    let head = &bytes[..bytes.len().min(32)];
    let end = head
        .iter()
        .position(|&b| b == b'\n' || b == b'\r')
        .unwrap_or(head.len());
    String::from_utf8_lossy(&head[..end]).trim().to_owned()
}

/// Decrypt a legacy encrypted `.brd` file into plain `.brd` bytes.
pub fn decrypt(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let Some(password) = password(bytes) else {
        return Err(Error::Invalid(format!(
            "unsupported encrypted .brd format {:?}",
            header_line(bytes)
        )));
    };
    let body = bytes.get(HEADER_LEN..).unwrap_or_default();
    if body.is_empty() || body.len() % 8 != 0 {
        return Err(Error::Invalid(
            "corrupt encrypted .brd file: the encrypted data is not a whole number of blocks"
                .into(),
        ));
    }
    cipher::decrypt(password, body)
}

#[cfg(feature = "legacy-encryption")]
mod cipher {
    use cbc::cipher::block_padding::Pkcs7;
    use cbc::cipher::{BlockDecryptMut, KeyIvInit};
    use md5::{Digest, Md5};

    use crate::Error;

    const SALT: [u8; 8] = [0xC7, 0x73, 0x21, 0x8C, 0x7E, 0xC8, 0xEE, 0x99];
    const ITERATIONS: usize = 20;

    /// PBKDF1 with MD5: the DES key and the CBC initialisation vector.
    pub(super) fn derive(password: &[u8]) -> ([u8; 8], [u8; 8]) {
        let mut d: [u8; 16] = Md5::new()
            .chain_update(password)
            .chain_update(SALT)
            .finalize()
            .into();
        for _ in 1..ITERATIONS {
            d = Md5::digest(d).into();
        }
        let mut key = [0; 8];
        let mut iv = [0; 8];
        key.copy_from_slice(&d[..8]);
        iv.copy_from_slice(&d[8..]);
        (key, iv)
    }

    /// Single DES in CBC mode with PKCS #5 padding.
    pub(super) fn decrypt(password: &[u8], body: &[u8]) -> Result<Vec<u8>, Error> {
        let (key, iv) = derive(password);
        let mut buf = body.to_vec();
        let n = cbc::Decryptor::<des::Des>::new(&key.into(), &iv.into())
            .decrypt_padded_mut::<Pkcs7>(&mut buf)
            .map_err(|_| Error::Invalid("corrupt encrypted .brd file: invalid padding".into()))?
            .len();
        buf.truncate(n);
        Ok(buf)
    }
}

#[cfg(not(feature = "legacy-encryption"))]
mod cipher {
    use crate::Error;

    pub(super) fn decrypt(_password: &[u8], _body: &[u8]) -> Result<Vec<u8>, Error> {
        Err(Error::Invalid(
            "encrypted .brd files are not supported by this build".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "legacy-encryption")]
    #[test]
    fn key_derivation_matches_the_test_vectors() {
        let hex = |b: [u8; 8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        let (k, iv) = cipher::derive(b"deltaXTaildeltaXMiddle");
        assert_eq!(hex(k), "8c41236a7fc96bce");
        assert_eq!(hex(iv), "7aff2c48277a18dd");
        let (k, iv) = cipher::derive(b"deltaXTail");
        assert_eq!(hex(k), "19cc4f0b1e99ef78");
        assert_eq!(hex(iv), "bb75f45c2eee1d93");
    }

    #[cfg(feature = "legacy-encryption")]
    #[test]
    fn decrypts_the_fixture_byte_for_byte() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures");
        let enc = std::fs::read(format!("{dir}/longboard_encrypted.brd")).unwrap();
        let plain = std::fs::read(format!("{dir}/longboard.brd")).unwrap();
        assert!(is_encrypted(&enc));
        assert_eq!(decrypt(&enc).unwrap(), plain);
        // Truncations fail cleanly (wrong length, or mostly wrong padding at block boundaries).
        for n in 0..enc.len() {
            let r = decrypt(&enc[..n]);
            assert!(r.is_err() || n % 8 == HEADER_LEN % 8);
            let _ = crate::brd::read(&enc[..n]);
        }
    }

    #[test]
    fn bad_files_are_errors() {
        let e = decrypt(b"%BRD-2.00  \nabcdefgh").unwrap_err();
        assert!(e.to_string().contains("unsupported"), "{e}");
        for bad in [
            &b"%BRD-1.02"[..],
            b"%BRD-1.02  \n",
            b"%BRD-1.02  \n1234567",
            b"%BRD-1.01  \n12345678",
        ] {
            assert!(decrypt(bad).is_err());
            assert!(crate::brd::read(bad).is_err());
        }
        // Random bodies never panic, neither here nor in the .brd reader behind.
        let mut seed = 3u64;
        for len in 0..120 {
            let mut v = b"%BRD-1.02  \n".to_vec();
            for _ in 0..len {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                v.push((seed >> 33) as u8);
            }
            let _ = decrypt(&v);
            let _ = crate::brd::read(&v);
        }
    }
}
