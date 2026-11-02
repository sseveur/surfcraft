//! Bug reports: a zip with the recent log, system information, the description and optionally the board and a
//! screenshot, written to the user's Downloads folder; then a prefilled GitHub issue or e-mail for the user to check
//! and send. SurfCraft itself never sends anything.

use std::path::{Path, PathBuf};

use crate::i18n::tr;

/// Where reports go: a new GitHub issue, or an e-mail.
pub const ISSUES_URL: &str = "https://github.com/sseveur/surfcraft/issues/new";
pub const REPORT_EMAIL: &str = "steven.seveur@gmail.com";
/// Browsers and GitHub accept long URLs, but stay well under the limits.
pub const MAX_ISSUE_URL: usize = 7000;
/// Some mail clients (and Windows' URL handler) cut mailto links around 2000 characters.
pub const MAX_MAILTO_URL: usize = 1800;

/// How far back the included log goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Since {
    Hour,
    Hours6,
    #[default]
    Day,
    Days3,
    Week,
    All,
}

impl Since {
    pub const ALL: [Since; 6] = [
        Since::Hour,
        Since::Hours6,
        Since::Day,
        Since::Days3,
        Since::Week,
        Since::All,
    ];

    pub fn seconds(self) -> Option<i64> {
        Some(match self {
            Since::Hour => 3600,
            Since::Hours6 => 6 * 3600,
            Since::Day => 24 * 3600,
            Since::Days3 => 3 * 86_400,
            Since::Week => 7 * 86_400,
            Since::All => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Since::Hour => tr("1 hour"),
            Since::Hours6 => tr("6 hours"),
            Since::Day => tr("24 hours"),
            Since::Days3 => tr("3 days"),
            Since::Week => tr("7 days"),
            Since::All => tr("All"),
        }
    }

    /// Unix time the log starts at, for a report made at `now`.
    pub fn start(self, now: i64) -> Option<i64> {
        self.seconds().map(|s| now - s)
    }
}

// ------------------------------------------------------------------------------------------------ URLs

/// Percent-encode everything but RFC 3986 unreserved characters.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn issue_url(title: &str, body: &str) -> String {
    format!("{ISSUES_URL}?title={}&body={}", encode(title), encode(body))
}

pub fn mailto_url(subject: &str, body: &str) -> String {
    format!(
        "mailto:{REPORT_EMAIL}?subject={}&body={}",
        encode(subject),
        encode(body)
    )
}

pub const SHORTENED: &str = "[... shortened: the full description is in report.txt in the zip]";

/// Message body `description` + `rest`, with the description shortened (and marked) until `make(body)` is at most
/// `max` characters long. Returns the URL and whether the description was shortened.
pub fn fit(
    description: &str,
    rest: &str,
    max: usize,
    make: impl Fn(&str) -> String,
) -> (String, bool) {
    let body = |d: &str| format!("{d}\n\n{rest}");
    let full = make(&body(description));
    if full.len() <= max {
        return (full, false);
    }
    let chars: Vec<(usize, char)> = description.char_indices().collect();
    let cut = |n: usize| {
        let end = chars.get(n).map_or(description.len(), |(i, _)| *i);
        make(&body(&format!(
            "{}\n{SHORTENED}",
            description[..end].trim_end()
        )))
    };
    // Longest prefix that fits (the URL grows with the prefix).
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if cut(mid).len() <= max {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    (cut(lo), true)
}

// ------------------------------------------------------------------------------------------------ contents

/// What goes into a report.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub title: String,
    pub description: String,
    pub email: String,
    pub system_info: Option<String>,
    pub logs: String,
    /// `.brd` text of the open board.
    pub board: Option<String>,
    pub screenshot_png: Option<Vec<u8>>,
}

impl Report {
    pub fn title_or_default(&self) -> String {
        let t = self.title.trim();
        if t.is_empty() {
            "Bug report".into()
        } else {
            crate::logging::redact(t)
        }
    }

    /// `report.txt`: what the user wrote and the system information.
    pub fn text(&self, made_at: i64) -> String {
        let mut s = format!(
            "{}\n\nCreated: {}\n",
            self.title_or_default(),
            crate::logging::rfc3339(made_at, 0)
        );
        if !self.email.trim().is_empty() {
            s += &format!("Contact: {}\n", self.email.trim());
        }
        s += &format!(
            "\nWhat happened / steps to reproduce:\n{}\n",
            crate::logging::redact(self.description.trim())
        );
        if let Some(info) = &self.system_info {
            s += &format!("\nSystem:\n{info}");
        }
        s
    }

    /// Message text for the issue or e-mail, without the description: system information and where the zip is.
    pub fn message_rest(&self, zip: &Path, markdown: bool) -> String {
        let name = zip
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let folder = zip.parent().map(crate::logging::path).unwrap_or_default();
        let mut s = String::new();
        if let Some(info) = &self.system_info {
            if markdown {
                s += &format!("**System**\n```\n{}```\n\n", info);
            } else {
                s += &format!("System:\n{info}\n");
            }
        }
        s += &format!(
            "Please attach the file {name} from {folder}: it holds the log{}.",
            if self.board.is_some() {
                " and the board"
            } else {
                ""
            }
        );
        s
    }

    pub fn description_or_default(&self) -> String {
        let d = crate::logging::redact(self.description.trim());
        if d.is_empty() {
            "(no description)".into()
        } else {
            d
        }
    }

    /// Prefilled "new issue" URL.
    pub fn issue_url(&self, zip: &Path) -> String {
        let title = self.title_or_default();
        fit(
            &self.description_or_default(),
            &self.message_rest(zip, true),
            MAX_ISSUE_URL,
            |body| issue_url(&title, body),
        )
        .0
    }

    /// Prefilled `mailto:` URL.
    pub fn mailto_url(&self, zip: &Path) -> String {
        let subject = format!("SurfCraft bug: {}", self.title_or_default());
        fit(
            &self.description_or_default(),
            &self.message_rest(zip, false),
            MAX_MAILTO_URL,
            |body| mailto_url(&subject, body),
        )
        .0
    }

    /// Files in the zip, in order.
    pub fn entries(&self, made_at: i64) -> Vec<(&'static str, Vec<u8>)> {
        let mut e = vec![("report.txt", self.text(made_at).into_bytes())];
        if let Some(info) = &self.system_info {
            e.push(("system-info.txt", info.clone().into_bytes()));
        }
        e.push(("surfcraft-log.txt", self.logs.clone().into_bytes()));
        if let Some(b) = &self.board {
            e.push(("board.brd", b.clone().into_bytes()));
        }
        if let Some(png) = &self.screenshot_png {
            e.push(("screenshot.png", png.clone()));
        }
        e
    }
}

/// `SurfCraft-bug-report-20261102-140305.zip` (UTC).
pub fn zip_name(secs: i64) -> String {
    let (y, m, d) = crate::logging::civil_from_days(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!(
        "SurfCraft-bug-report-{y:04}{m:02}{d:02}-{:02}{:02}{:02}.zip",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

/// Downloads, else Desktop, else the data folder.
pub fn output_dir(data_dir: Option<&Path>) -> Option<PathBuf> {
    let user = directories::UserDirs::new();
    let candidates = [
        user.as_ref()
            .and_then(|u| u.download_dir())
            .map(Path::to_path_buf),
        user.as_ref()
            .and_then(|u| u.desktop_dir())
            .map(Path::to_path_buf),
        data_dir.map(Path::to_path_buf),
    ];
    candidates.into_iter().flatten().find(|d| d.is_dir())
}

/// Write the report zip into `dir` (temp file, then rename). Returns its path.
pub fn write(dir: &Path, report: &Report, made_at: i64) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(zip_name(made_at));
    let entries = report.entries(made_at);
    let refs: Vec<(&str, &[u8])> = entries.iter().map(|(n, b)| (*n, b.as_slice())).collect();
    let tmp = dir.join(format!(".{}.tmp", zip_name(made_at)));
    std::fs::write(&tmp, zip::store(&refs, made_at))?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Locale of the user's system, as the environment or macOS reports it.
pub fn locale() -> String {
    let env = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty());
    if let Some(l) = env {
        return l;
    }
    #[cfg(all(target_os = "macos", not(test)))]
    if let Ok(out) = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
        && out.status.success()
    {
        return String::from_utf8_lossy(&out.stdout).trim().to_owned();
    }
    "unknown".into()
}

// ------------------------------------------------------------------------------------------------ zip

/// Minimal zip writer: stored (uncompressed) entries, UTF-8 names. Logs and boards are small.
pub mod zip {
    const CRC_TABLE: [u32; 256] = {
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

    pub fn crc32(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        !c
    }

    /// MS-DOS time and date of Unix seconds (UTC).
    pub fn dos_time(secs: i64) -> (u16, u16) {
        let (y, m, d) = crate::logging::civil_from_days(secs.div_euclid(86_400));
        let s = secs.rem_euclid(86_400);
        let time = ((s / 3600) << 11) | ((s / 60 % 60) << 5) | ((s % 60) / 2);
        let date = (((y - 1980).clamp(0, 127) as u32) << 9) | (m << 5) | d;
        (time as u16, date as u16)
    }

    const UTF8_NAMES: u16 = 1 << 11;

    /// Zip archive of `(name, bytes)` entries, all dated `secs`.
    pub fn store(entries: &[(&str, &[u8])], secs: i64) -> Vec<u8> {
        let (time, date) = dos_time(secs);
        let mut out = Vec::new();
        let mut central = Vec::new();
        let u16le = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
        let u32le = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
        for (name, data) in entries {
            let crc = crc32(data);
            let offset = out.len() as u32;
            // Local file header.
            u32le(&mut out, 0x0403_4b50);
            for x in [20, UTF8_NAMES, 0, time, date] {
                u16le(&mut out, x);
            }
            for x in [crc, data.len() as u32, data.len() as u32] {
                u32le(&mut out, x);
            }
            u16le(&mut out, name.len() as u16);
            u16le(&mut out, 0);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            // Central directory entry.
            u32le(&mut central, 0x0201_4b50);
            for x in [20, 20, UTF8_NAMES, 0, time, date] {
                u16le(&mut central, x);
            }
            for x in [crc, data.len() as u32, data.len() as u32] {
                u32le(&mut central, x);
            }
            for x in [name.len() as u16, 0, 0, 0, 0] {
                u16le(&mut central, x);
            }
            u32le(&mut central, 0);
            u32le(&mut central, offset);
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&central);
        u32le(&mut out, 0x0605_4b50);
        for x in [0, 0, entries.len() as u16, entries.len() as u16] {
            u16le(&mut out, x);
        }
        u32le(&mut out, central.len() as u32);
        u32le(&mut out, cd_offset);
        u16le(&mut out, 0);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read a stored zip back through its central directory, checking every header and CRC.
    fn unzip(z: &[u8]) -> Vec<(String, Vec<u8>)> {
        let u16at = |i: usize| u16::from_le_bytes([z[i], z[i + 1]]) as usize;
        let u32at = |i: usize| u32::from_le_bytes(z[i..i + 4].try_into().unwrap()) as usize;
        let eocd = z.len() - 22;
        assert_eq!(u32at(eocd), 0x0605_4b50);
        let (count, cd_size, cd_offset) = (u16at(eocd + 10), u32at(eocd + 12), u32at(eocd + 16));
        assert_eq!(cd_offset + cd_size, eocd);
        let mut out = Vec::new();
        let mut p = cd_offset;
        for _ in 0..count {
            assert_eq!(u32at(p), 0x0201_4b50);
            let (method, crc, size) = (u16at(p + 10), u32at(p + 16), u32at(p + 24));
            assert_eq!(method, 0, "stored");
            assert_eq!(size, u32at(p + 20));
            let name_len = u16at(p + 28);
            let name = String::from_utf8(z[p + 46..p + 46 + name_len].to_vec()).unwrap();
            let local = u32at(p + 42);
            assert_eq!(u32at(local), 0x0403_4b50);
            assert_eq!(u32at(local + 14), crc);
            let data_at = local + 30 + u16at(local + 26) + u16at(local + 28);
            let data = z[data_at..data_at + size].to_vec();
            assert_eq!(zip::crc32(&data) as usize, crc);
            out.push((name, data));
            p += 46 + name_len + u16at(p + 30) + u16at(p + 32);
        }
        assert_eq!(p, eocd);
        out
    }

    #[test]
    fn crc_and_dos_time() {
        assert_eq!(zip::crc32(b""), 0);
        assert_eq!(zip::crc32(b"123456789"), 0xCBF4_3926);
        // 2026-11-02 14:03:06
        let t = crate::logging::days_from_civil(2026, 11, 2) * 86_400 + 14 * 3600 + 3 * 60 + 6;
        let (time, date) = zip::dos_time(t);
        assert_eq!(time, 14 << 11 | 3 << 5 | 3);
        assert_eq!(date, 46 << 9 | 11 << 5 | 2);
        assert_eq!(zip_name(t), "SurfCraft-bug-report-20261102-140306.zip");
    }

    #[test]
    fn zip_round_trip() {
        let big: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let entries: Vec<(&str, &[u8])> = vec![
            ("report.txt", b"hello"),
            ("empty.txt", b""),
            ("planche-\u{e9}t\u{e9}.brd", &big),
        ];
        let back = unzip(&zip::store(&entries, 1_800_000_000));
        assert_eq!(back.len(), 3);
        for ((n, d), (bn, bd)) in entries.iter().zip(&back) {
            assert_eq!(n, bn);
            assert_eq!(*d, bd.as_slice());
        }
        assert_eq!(unzip(&zip::store(&[], 0)), vec![]);
    }

    #[test]
    fn report_zip_holds_the_chosen_files() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/unit-tmp/bug-zip");
        let _ = std::fs::remove_dir_all(&dir);
        let mut r = Report {
            title: "Crash".into(),
            description: "Clicked flip".into(),
            email: "a@b.c".into(),
            logs: "2026-11-02T10:00:00.000Z INFO  surfcraft: x\n".into(),
            ..Default::default()
        };
        let p = write(&dir, &r, 1_800_000_000).unwrap();
        let names: Vec<String> = unzip(&std::fs::read(&p).unwrap())
            .into_iter()
            .map(|e| e.0)
            .collect();
        assert_eq!(names, ["report.txt", "surfcraft-log.txt"]);
        r.system_info = Some("SurfCraft 4\n".into());
        r.board = Some("p00 : x\n".into());
        r.screenshot_png = Some(vec![1, 2, 3]);
        let files = unzip(&std::fs::read(write(&dir, &r, 1_800_000_001).unwrap()).unwrap());
        let names: Vec<&str> = files.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(
            names,
            [
                "report.txt",
                "system-info.txt",
                "surfcraft-log.txt",
                "board.brd",
                "screenshot.png"
            ]
        );
        let text = String::from_utf8(files[0].1.clone()).unwrap();
        assert!(
            text.contains("Crash")
                && text.contains("Contact: a@b.c")
                && text.contains("Clicked flip")
        );
        // No temp files left behind.
        let leftovers = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn encoding() {
        assert_eq!(encode("a b&c=d/é?#~"), "a%20b%26c%3Dd%2F%C3%A9%3F%23~");
        assert_eq!(
            issue_url("T 1", "x\ny"),
            "https://github.com/sseveur/surfcraft/issues/new?title=T%201&body=x%0Ay"
        );
        assert!(mailto_url("s", "b").starts_with("mailto:steven.seveur@gmail.com?subject=s&body="));
    }

    #[test]
    fn long_descriptions_are_shortened_to_fit_the_url() {
        let zip = Path::new("/tmp/SurfCraft-bug-report-20261102-140306.zip");
        let mut r = Report {
            title: "Slow".into(),
            description: "short".into(),
            system_info: Some("SurfCraft 4\nSystem: macos aarch64\n".into()),
            logs: "LOG LINE THAT MUST NOT BE IN THE URL".into(),
            ..Default::default()
        };
        let url = r.issue_url(zip);
        assert!(url.len() < MAX_ISSUE_URL);
        assert!(url.contains("short") && !url.contains("LOG%20LINE"));
        assert!(url.contains(&encode("SurfCraft-bug-report-20261102-140306.zip")));
        // Multi-byte text: the cut stays on a character boundary and the marker shows.
        r.description = "Planche \u{e9}trange. ".repeat(2000);
        let url = r.issue_url(zip);
        assert!(
            url.len() <= MAX_ISSUE_URL && url.len() > MAX_ISSUE_URL - 200,
            "{}",
            url.len()
        );
        assert!(url.contains(&encode(SHORTENED)));
        assert!(
            url.contains(&encode("System: macos aarch64")),
            "system info kept"
        );
        let mail = r.mailto_url(zip);
        assert!(mail.len() <= MAX_MAILTO_URL, "{}", mail.len());
        assert!(mail.contains(&encode(SHORTENED)));
        // The full text still goes into the zip.
        assert!(r.text(0).contains(&r.description.trim().to_string()));
        // The contact address never goes into the public issue.
        r.email = "me@example.com".into();
        assert!(!r.issue_url(zip).contains("example.com"));
    }

    #[test]
    fn fit_keeps_short_bodies_whole() {
        let (u, cut) = fit("abc", "rest", 100, |b| b.to_owned());
        assert_eq!((u.as_str(), cut), ("abc\n\nrest", false));
        let (u, cut) = fit(&"x".repeat(500), "rest", 100, |b| b.to_owned());
        assert!(cut && u.len() <= 100 && u.ends_with("rest") && u.contains(SHORTENED));
    }

    #[test]
    fn since_windows() {
        assert_eq!(Since::default(), Since::Day);
        assert_eq!(Since::Hour.start(10_000), Some(6400));
        assert_eq!(Since::Week.seconds(), Some(604_800));
        assert_eq!(Since::All.start(10_000), None);
    }
}
