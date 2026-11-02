//! Interface translations. The English text is the key: `tr("Volume")` returns the current language's entry from
//! `i18n/<code>.txt` (lines `English text = Translation`, `#` comments), or the English text when there is none.
//! Formatted strings use positional placeholders: `trf("wide point {0}", &[&width])`.

use std::collections::HashMap;
use std::fmt::{Display, Write as _};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    #[default]
    En,
    Fr,
    Es,
    Pt,
    Nl,
    No,
}

impl Language {
    pub const ALL: [Language; 6] = [
        Language::En,
        Language::Fr,
        Language::Es,
        Language::Pt,
        Language::Nl,
        Language::No,
    ];

    #[cfg(test)]
    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Fr => "fr",
            Language::Es => "es",
            Language::Pt => "pt",
            Language::Nl => "nl",
            Language::No => "no",
        }
    }

    /// The language's own name, shown untranslated in the picker.
    pub fn native_name(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::Fr => "Français",
            Language::Es => "Español",
            Language::Pt => "Português",
            Language::Nl => "Nederlands",
            Language::No => "Norsk bokmål",
        }
    }

    /// Language of a POSIX/Apple locale name such as `fr_FR.UTF-8`, `pt-BR` or `nb_NO`.
    pub fn from_locale(locale: &str) -> Option<Language> {
        let lang = locale
            .split(['_', '-', '.', '@'])
            .next()?
            .to_ascii_lowercase();
        Some(match lang.as_str() {
            "en" => Language::En,
            "fr" => Language::Fr,
            "es" => Language::Es,
            "pt" => Language::Pt,
            "nl" => Language::Nl,
            "no" | "nb" => Language::No,
            _ => return None,
        })
    }

    fn source(self) -> &'static str {
        match self {
            Language::En => "",
            Language::Fr => include_str!("../i18n/fr.txt"),
            Language::Es => include_str!("../i18n/es.txt"),
            Language::Pt => include_str!("../i18n/pt.txt"),
            Language::Nl => include_str!("../i18n/nl.txt"),
            Language::No => include_str!("../i18n/no.txt"),
        }
    }

    fn index(self) -> usize {
        Language::ALL.iter().position(|&l| l == self).unwrap_or(0)
    }
}

// The app has one UI thread, so the language is process-wide. Tests run many app instances on parallel test threads;
// there each thread keeps its own language so a test that switches languages cannot affect the others.
#[cfg(not(test))]
static CURRENT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
thread_local! {
    static CURRENT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub fn language() -> Language {
    #[cfg(not(test))]
    let i = CURRENT.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    let i = CURRENT.with(std::cell::Cell::get);
    Language::ALL[i]
}

pub fn set_language(lang: Language) {
    #[cfg(not(test))]
    CURRENT.store(lang.index(), std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    CURRENT.with(|c| c.set(lang.index()));
}

/// Language of the operating system, or English when it is not one of ours. Tests always get English.
pub fn system_language() -> Language {
    if cfg!(test) {
        return Language::En;
    }
    // POSIX precedence: the first non-empty variable decides (also honoured on Windows, e.g. from MSYS shells).
    let env = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty());
    if let Some(l) = env.as_deref().and_then(Language::from_locale) {
        return l;
    }
    #[cfg(windows)]
    {
        // The user's locale, e.g. "fr-FR".
        let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
        // SAFETY: the buffer and its length in UTF-16 units are valid for the call.
        let n = unsafe {
            windows_sys::Win32::Globalization::GetUserDefaultLocaleName(
                buf.as_mut_ptr(),
                buf.len() as i32,
            )
        };
        if n > 1
            && let Some(l) =
                Language::from_locale(&String::from_utf16_lossy(&buf[..n as usize - 1]))
        {
            return l;
        }
    }
    #[cfg(target_os = "macos")]
    {
        // Apps started from Finder get no LANG; ask for the user's locale (e.g. "fr_FR").
        if let Ok(out) = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleLocale"])
            .output()
            && out.status.success()
            && let Some(l) = Language::from_locale(String::from_utf8_lossy(&out.stdout).trim())
        {
            return l;
        }
    }
    Language::En
}

/// Parse `English = Translation` lines; blank lines and `#` comments are skipped.
fn parse(src: &'static str) -> HashMap<&'static str, &'static str> {
    src.lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once(" = "))
        .collect()
}

fn tables() -> &'static [HashMap<&'static str, &'static str>] {
    static TABLES: OnceLock<Vec<HashMap<&'static str, &'static str>>> = OnceLock::new();
    TABLES.get_or_init(|| Language::ALL.iter().map(|l| parse(l.source())).collect())
}

/// `english` in the current language (English when there is no translation).
pub fn tr(english: &'static str) -> &'static str {
    let lang = language();
    if lang == Language::En {
        return english;
    }
    tables()[lang.index()]
        .get(english)
        .copied()
        .unwrap_or(english)
}

/// Translate `english`, then replace `{0}`, `{1}`, … with `args`.
pub fn trf(english: &'static str, args: &[&dyn Display]) -> String {
    fill(tr(english), args)
}

fn fill(template: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let arg = after.find('}').and_then(|close| {
            let n: usize = after[..close].parse().ok()?;
            Some((args.get(n)?, close))
        });
        match arg {
            Some((a, close)) => {
                let _ = write!(out, "{a}");
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Keys translated at run time rather than written as `tr("…")` literals: names from other crates' tables.
#[cfg(test)]
pub(crate) fn dynamic_keys() -> Vec<&'static str> {
    use crate::edit::CurveId;
    let mut keys = Vec::new();
    for t in surfcraft_io::templates::TEMPLATES {
        keys.push(t.name);
        keys.push(t.description);
    }
    keys.extend(
        surfcraft_export::woodwork::TEMPLATES
            .iter()
            .map(|(_, l)| *l),
    );
    keys.extend(
        [
            CurveId::Outline,
            CurveId::OutlineLeft,
            CurveId::Deck,
            CurveId::Bottom,
            CurveId::Section(0),
        ]
        .map(CurveId::label),
    );
    // Unit symbols (mm, cm, m) and ISO paper sizes stay as they are.
    keys.extend(
        surfcraft_core::LengthUnit::ALL
            .iter()
            .map(|u| u.label())
            .filter(|l| l.contains('(')),
    );
    keys.extend(
        surfcraft_export::drawing::Paper::ALL
            .iter()
            .map(|p| p.label())
            .filter(|l| l.starts_with("US")),
    );
    keys
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// Every `tr("…")` / `trf("…")` literal in `src/*.rs` (other than this file).
    fn source_keys() -> BTreeSet<String> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut keys = BTreeSet::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("i18n.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            for call in ["tr(", "trf("] {
                for (at, _) in src.match_indices(call) {
                    let before = src[..at].chars().next_back();
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    // rustfmt may break the line after the parenthesis.
                    if let Some(arg) = src[at + call.len()..].trim_start().strip_prefix('"') {
                        keys.insert(literal(arg));
                    }
                }
            }
        }
        keys
    }

    /// The contents of a Rust string literal whose opening quote has been consumed.
    fn literal(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return out,
                '\\' => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('\n') => {
                        // Line continuation: skip the next line's indentation.
                        let rest: String = chars.collect();
                        return out + &literal(rest.trim_start());
                    }
                    Some(other) => out.push(other),
                    None => break,
                },
                c => out.push(c),
            }
        }
        panic!("unterminated string literal");
    }

    fn placeholders(s: &str) -> BTreeSet<usize> {
        let mut set = BTreeSet::new();
        let mut rest = s;
        while let Some(open) = rest.find('{') {
            rest = &rest[open + 1..];
            if let Some(close) = rest.find('}')
                && let Ok(n) = rest[..close].parse()
            {
                set.insert(n);
            }
        }
        set
    }

    /// Characters the default egui font (Ubuntu-Light) draws: Latin-1 and the punctuation the English text uses.
    fn renders(c: char) -> bool {
        (c as u32) < 0x100 || "…·—–×°²’".contains(c)
    }

    #[test]
    fn tables_match_the_source() {
        let used = source_keys();
        assert!(used.len() > 100, "found only {} keys", used.len());
        let dynamic: BTreeSet<String> = dynamic_keys().into_iter().map(String::from).collect();
        let mut problems = Vec::new();
        for key in used.iter().chain(&dynamic) {
            if key.starts_with(' ') || key.ends_with(' ') {
                problems.push(format!("key {key:?} has outer spaces (lost when parsing)"));
            }
        }
        for lang in &Language::ALL[1..] {
            let code = lang.code();
            let src = lang.source();
            let mut entries = 0;
            for (n, line) in src.lines().enumerate() {
                let line = line.trim_end_matches('\r');
                if line.trim().is_empty() || line.trim_start().starts_with('#') {
                    continue;
                }
                entries += 1;
                if line.split(" = ").count() != 2 {
                    problems.push(format!(
                        "{code}.txt:{}: expected one ' = ': {line:?}",
                        n + 1
                    ));
                }
            }
            let table = parse(src);
            if table.len() != entries {
                problems.push(format!("{code}.txt: duplicate keys"));
            }
            for key in used.iter().chain(&dynamic) {
                if !table.contains_key(key.as_str()) {
                    problems.push(format!("{code}.txt: missing {key:?}"));
                }
            }
            for (key, value) in &table {
                if !used.contains(*key) && !dynamic.contains(*key) {
                    problems.push(format!("{code}.txt: unused {key:?}"));
                }
                if placeholders(key) != placeholders(value) {
                    problems.push(format!("{code}.txt: placeholders differ in {key:?}"));
                }
                if value.trim().is_empty() || value.trim() != *value {
                    problems.push(format!(
                        "{code}.txt: empty or padded translation of {key:?}"
                    ));
                }
                if let Some(c) = value.chars().find(|&c| !renders(c)) {
                    problems.push(format!("{code}.txt: {c:?} has no glyph in {value:?}"));
                }
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn lookup_falls_back_and_fills_placeholders() {
        set_language(Language::Fr);
        assert_eq!(tr("Volume"), "Volume");
        assert_eq!(tr("no such text"), "no such text");
        assert_ne!(tr("Thickness"), "Thickness");
        set_language(Language::En);
        assert_eq!(tr("Thickness"), "Thickness");
        assert_eq!(fill("{1} of {0}", &[&1, &"x"]), "x of 1");
        assert_eq!(fill("{a} {9} {", &[&1]), "{a} {9} {");
    }

    #[test]
    fn locale_names() {
        for lang in Language::ALL {
            assert_eq!(Language::from_locale(lang.code()), Some(lang));
        }
        assert_eq!(Language::from_locale("fr_FR.UTF-8"), Some(Language::Fr));
        assert_eq!(Language::from_locale("pt-BR"), Some(Language::Pt));
        assert_eq!(Language::from_locale("nb_NO"), Some(Language::No));
        assert_eq!(Language::from_locale("en_GB"), Some(Language::En));
        assert_eq!(Language::from_locale("de_DE"), None);
        assert_eq!(Language::from_locale("C"), None);
    }
}
