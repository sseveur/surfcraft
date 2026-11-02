//! Minimal ZIP archives: stored entries (no compression), CRC-32, no ZIP64 (each entry and the whole archive
//! must stay under 4 GiB). Enough for 3MF packages and bundling a few text files.
//!
//! [`read`] reads back archives of stored entries (such as the ones [`ZipWriter`] makes) and checks every CRC.

/// CRC-32 (IEEE 802.3, reflected polynomial 0xEDB88320), as used by ZIP and PNG.
pub fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut t = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            t[i] = c;
            i += 1;
        }
        t
    };
    let mut c = !0u32;
    for &b in data {
        c = TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const END_SIG: u32 = 0x0605_4b50;
/// General purpose flag bit 11: file names are UTF-8.
const UTF8_FLAG: u16 = 1 << 11;
/// DOS date 1980-01-01, time 00:00 (fixed, so archives are reproducible).
const DOS_DATE: u16 = (1 << 5) | 1;
const DOS_TIME: u16 = 0;

struct Entry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

/// Builds a ZIP archive in memory.
#[derive(Default)]
pub struct ZipWriter {
    out: Vec<u8>,
    entries: Vec<Entry>,
}

fn u16le(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn u32le(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

impl ZipWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file (stored, uncompressed). `name` uses `/` separators, without a leading `/`.
    ///
    /// Panics if the archive would exceed 4 GiB (ZIP64 is not supported).
    pub fn add(&mut self, name: &str, data: &[u8]) {
        let size = u32::try_from(data.len()).expect("zip entry over 4 GiB");
        let offset = u32::try_from(self.out.len()).expect("zip archive over 4 GiB");
        let crc = crc32(data);
        let o = &mut self.out;
        u32le(o, LOCAL_SIG);
        u16le(o, 20); // version needed: 2.0
        u16le(o, UTF8_FLAG);
        u16le(o, 0); // method: stored
        u16le(o, DOS_TIME);
        u16le(o, DOS_DATE);
        u32le(o, crc);
        u32le(o, size); // compressed size
        u32le(o, size);
        u16le(o, name.len() as u16);
        u16le(o, 0); // extra field length
        o.extend_from_slice(name.as_bytes());
        o.extend_from_slice(data);
        self.entries.push(Entry {
            name: name.to_owned(),
            crc,
            size,
            offset,
        });
    }

    /// Write the central directory and return the archive bytes.
    pub fn finish(mut self) -> Vec<u8> {
        let start = self.out.len() as u32;
        let o = &mut self.out;
        for e in &self.entries {
            u32le(o, CENTRAL_SIG);
            u16le(o, 20); // version made by
            u16le(o, 20); // version needed
            u16le(o, UTF8_FLAG);
            u16le(o, 0);
            u16le(o, DOS_TIME);
            u16le(o, DOS_DATE);
            u32le(o, e.crc);
            u32le(o, e.size);
            u32le(o, e.size);
            u16le(o, e.name.len() as u16);
            u16le(o, 0); // extra
            u16le(o, 0); // comment
            u16le(o, 0); // disk number
            u16le(o, 0); // internal attributes
            u32le(o, 0); // external attributes
            u32le(o, e.offset);
            o.extend_from_slice(e.name.as_bytes());
        }
        let size = o.len() as u32 - start;
        u32le(o, END_SIG);
        u16le(o, 0);
        u16le(o, 0);
        u16le(o, self.entries.len() as u16);
        u16le(o, self.entries.len() as u16);
        u32le(o, size);
        u32le(o, start);
        u16le(o, 0); // comment length
        self.out
    }
}

/// Convenience: an archive of `(name, data)` entries in order.
pub fn write(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = ZipWriter::new();
    for (name, data) in entries {
        z.add(name, data);
    }
    z.finish()
}

fn rd16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn rd32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// Read an archive of stored entries through its central directory: `(name, data)` in directory order.
/// Fails on compressed entries, a bad CRC or a truncated archive.
pub fn read(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let bad = || "truncated or invalid zip".to_string();
    // The end record is at least 22 bytes and may be followed by a comment of up to 64 KiB.
    let min = bytes.len().saturating_sub(22 + 0xFFFF);
    let end = (min..=bytes.len().saturating_sub(22))
        .rev()
        .find(|&i| rd32(bytes, i) == Some(END_SIG))
        .ok_or("no end of central directory")?;
    let count = rd16(bytes, end + 10).ok_or_else(bad)? as usize;
    let mut p = rd32(bytes, end + 16).ok_or_else(bad)? as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if rd32(bytes, p) != Some(CENTRAL_SIG) {
            return Err("bad central directory entry".into());
        }
        let method = rd16(bytes, p + 10).ok_or_else(bad)?;
        let crc = rd32(bytes, p + 16).ok_or_else(bad)?;
        let size = rd32(bytes, p + 20).ok_or_else(bad)? as usize;
        let name_len = rd16(bytes, p + 28).ok_or_else(bad)? as usize;
        let extra_len = rd16(bytes, p + 30).ok_or_else(bad)? as usize;
        let comment_len = rd16(bytes, p + 32).ok_or_else(bad)? as usize;
        let offset = rd32(bytes, p + 42).ok_or_else(bad)? as usize;
        let name = String::from_utf8(
            bytes
                .get(p + 46..p + 46 + name_len)
                .ok_or_else(bad)?
                .to_vec(),
        )
        .map_err(|_| "file name is not UTF-8")?;
        if method != 0 {
            return Err(format!("{name}: compressed entries are not supported"));
        }
        if rd32(bytes, offset) != Some(LOCAL_SIG) {
            return Err(format!("{name}: bad local header"));
        }
        let local_name = rd16(bytes, offset + 26).ok_or_else(bad)? as usize;
        let local_extra = rd16(bytes, offset + 28).ok_or_else(bad)? as usize;
        let start = offset + 30 + local_name + local_extra;
        let data = bytes.get(start..start + size).ok_or_else(bad)?;
        if crc32(data) != crc {
            return Err(format!("{name}: CRC mismatch"));
        }
        out.push((name, data.to_vec()));
        p += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_values() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
    }

    #[test]
    fn round_trip() {
        let big: Vec<u8> = (0..100_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let z = write(&[
            ("a.txt", b"hello"),
            ("dir/empty", b""),
            ("dir/big.bin", &big),
            ("café.txt", "ü".as_bytes()),
        ]);
        let entries = read(&z).unwrap();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0], ("a.txt".into(), b"hello".to_vec()));
        assert_eq!(entries[1].1.len(), 0);
        assert_eq!(entries[2].1, big);
        assert_eq!(entries[3].0, "café.txt");
        // End record: 22 bytes at the very end, entry counts and directory offset consistent.
        let end = z.len() - 22;
        assert_eq!(rd32(&z, end), Some(END_SIG));
        assert_eq!(rd16(&z, end + 10), Some(4));
        let dir_size = rd32(&z, end + 12).unwrap() as usize;
        let dir_start = rd32(&z, end + 16).unwrap() as usize;
        assert_eq!(dir_start + dir_size, end);
    }

    #[test]
    fn detects_corruption() {
        let mut z = write(&[("a.txt", b"hello world")]);
        z[30 + 5 + 2] ^= 0xFF; // a byte of the data
        assert!(read(&z).unwrap_err().contains("CRC"));
        assert!(read(b"not a zip").is_err());
        assert!(read(&[]).is_err());
    }
}
