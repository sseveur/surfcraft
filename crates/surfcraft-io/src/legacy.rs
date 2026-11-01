//! Placeholder: decryption of legacy `%BRD-` files is written by the file-format implementer; only the API used by callers is declared here.

use crate::Error;

/// Whether the bytes start with the header of a legacy encrypted `.brd` file.
pub fn is_encrypted(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%BRD-")
}

/// Decrypt a legacy encrypted `.brd` file into plain `.brd` bytes.
pub fn decrypt(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let _ = bytes;
    unimplemented!("legacy .brd decryption not written yet")
}
