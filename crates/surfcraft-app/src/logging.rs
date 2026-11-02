//! App-wide log: one file per day in `<data dir>/logs/surfcraft-YYYY-MM-DD.log`, every line starting with an
//! RFC 3339 UTC timestamp so a bug report can include just the last hours. Files older than [`KEEP_DAYS`] are
//! deleted at startup. The home directory is written as `~`. Nothing here ever leaves the computer by itself.
//!
//! "Detailed logging" (Help menu) adds debug-level events: edits, undo/redo, clicks, view and tool changes.

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Log files kept, today included.
pub const KEEP_DAYS: i64 = 14;
const PREFIX: &str = "surfcraft-";
const SUFFIX: &str = ".log";

// ------------------------------------------------------------------------------------------------ time

/// Seconds and milliseconds since the Unix epoch.
pub fn now() -> (i64, u32) {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (d.as_secs() as i64, d.subsec_millis())
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Date of a day number from [`days_from_civil`].
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `2026-11-02T14:03:05.123Z`.
pub fn rfc3339(secs: i64, millis: u32) -> String {
    let (y, mo, d) = civil_from_days(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

fn digits(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Day number of `YYYY-MM-DD`.
pub fn parse_date(s: &str) -> Option<i64> {
    if s.len() != 10 || &s[4..5] != "-" || &s[7..8] != "-" {
        return None;
    }
    let (y, m, d) = (digits(&s[..4])?, digits(&s[5..7])?, digits(&s[8..10])?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y as i64, m, d))
}

/// Unix seconds of the timestamp a log line starts with (`YYYY-MM-DDTHH:MM:SS…`), if it has one.
pub fn line_time(line: &str) -> Option<i64> {
    let head = line.get(..19)?;
    if &head[10..11] != "T" || &head[13..14] != ":" || &head[16..17] != ":" {
        return None;
    }
    let days = parse_date(&head[..10])?;
    let (h, m, s) = (
        digits(&head[11..13])?,
        digits(&head[14..16])?,
        digits(&head[17..19])?,
    );
    Some(days * 86_400 + (h * 3600 + m * 60 + s) as i64)
}

// ------------------------------------------------------------------------------------------------ files

/// Base folder for logs and autosaves, next to the settings (`~/Library/Application Support/org.surfcraft.SurfCraft`
/// on macOS). `SURFCRAFT_DATA_DIR` overrides it (smoke tests, several instances).
pub fn data_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("SURFCRAFT_DATA_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    eframe::storage_dir(crate::APP_ID)
}

pub fn log_dir(base: &Path) -> PathBuf {
    base.join("logs")
}

pub fn file_name(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{PREFIX}{y:04}-{m:02}-{d:02}{SUFFIX}")
}

/// Day of a log file name, `None` for anything else.
pub fn file_day(name: &str) -> Option<i64> {
    parse_date(name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?)
}

/// Log files in `dir`, oldest first.
pub fn log_files(dir: &Path) -> Vec<(i64, PathBuf)> {
    let mut files: Vec<(i64, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let day = file_day(&e.file_name().to_string_lossy())?;
            e.file_type().ok()?.is_file().then(|| (day, e.path()))
        })
        .collect();
    files.sort();
    files
}

/// Delete log files older than `keep` days (today counts as one). Only touches files named like ours, in `dir`.
pub fn cleanup(dir: &Path, today: i64, keep: i64) -> usize {
    let mut removed = 0;
    for (day, path) in log_files(dir) {
        if today - day >= keep && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Keep the lines at or after `since` (Unix seconds; `None` keeps all). Lines without a timestamp (continuation
/// lines of a message, such as a backtrace) go with the line before them.
pub fn filter_since(text: &str, since: Option<i64>) -> String {
    let mut out = String::new();
    let mut keep = since.is_none();
    for line in text.lines() {
        if let Some(t) = line_time(line) {
            keep = since.is_none_or(|s| t >= s);
        }
        if keep {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Log text from all files in `dir` at or after `since`, oldest first, home folder shown as `~`.
pub fn collect(dir: &Path, since: Option<i64>) -> String {
    let mut out = String::new();
    for (day, path) in log_files(dir) {
        if since.is_some_and(|s| (day + 1) * 86_400 <= s) {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            out.push_str(&filter_since(&String::from_utf8_lossy(&bytes), since));
        }
    }
    redact(&out)
}

// ------------------------------------------------------------------------------------------------ redaction

fn home() -> Option<&'static str> {
    static HOME: OnceLock<Option<String>> = OnceLock::new();
    HOME.get_or_init(|| {
        directories::BaseDirs::new()
            .map(|b| b.home_dir().to_string_lossy().into_owned())
            .filter(|h| h.len() > 1)
    })
    .as_deref()
}

/// Replace the home folder with `~`.
pub fn redact(s: &str) -> String {
    match home() {
        Some(h) => redact_with(s, h),
        None => s.to_owned(),
    }
}

pub fn redact_with(s: &str, home: &str) -> String {
    let home = home.trim_end_matches(['/', '\\']);
    if home.len() < 2 {
        return s.to_owned();
    }
    s.replace(home, "~")
}

/// A path for the log, home folder as `~`.
pub fn path(p: &Path) -> String {
    redact(&p.display().to_string())
}

// ------------------------------------------------------------------------------------------------ logger

// Process-wide in the app; per test thread in tests, like the interface language (tests run in parallel).
#[cfg(not(test))]
static DETAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
thread_local! {
    static DETAILED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn swap_detailed(on: bool) -> bool {
    #[cfg(not(test))]
    return DETAILED.swap(on, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    return DETAILED.with(|d| d.replace(on));
}
static GPU: OnceLock<String> = OnceLock::new();

/// Turn debug-level events on or off.
pub fn set_detailed(on: bool) {
    if swap_detailed(on) != on {
        log::set_max_level(if on {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        });
        if on {
            log::info!("detailed logging on");
        }
    }
}

pub fn detailed() -> bool {
    #[cfg(not(test))]
    return DETAILED.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    return DETAILED.with(std::cell::Cell::get);
}

/// Graphics adapter, recorded once at startup for the log and bug reports.
pub fn set_gpu(info: String) {
    let _ = GPU.set(info);
}

pub fn gpu() -> &'static str {
    GPU.get().map(String::as_str).unwrap_or("unknown")
}

struct FileLogger {
    dir: PathBuf,
    /// Day number and open file.
    file: Mutex<Option<(i64, File)>>,
}

impl FileLogger {
    fn write(&self, line: &str) {
        let (secs, _) = now();
        let day = secs.div_euclid(86_400);
        let Ok(mut slot) = self.file.lock() else {
            return;
        };
        if slot.as_ref().is_none_or(|(d, _)| *d != day) {
            *slot = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.dir.join(file_name(day)))
                .ok()
                .map(|f| (day, f));
        }
        if let Some((_, f)) = slot.as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

/// One log line; continuation lines of a multi-line message are indented.
pub fn format_line(secs: i64, millis: u32, level: log::Level, target: &str, msg: &str) -> String {
    let mut line = format!("{} {level:<5} {target}: ", rfc3339(secs, millis));
    for (i, part) in msg.lines().enumerate() {
        if i > 0 {
            line.push_str("\n    ");
        }
        line.push_str(part);
    }
    line.push('\n');
    line
}

fn ours(target: &str) -> bool {
    target.starts_with("surfcraft")
}

impl log::Log for FileLogger {
    fn enabled(&self, meta: &log::Metadata) -> bool {
        let max = if !ours(meta.target()) {
            log::Level::Warn
        } else if detailed() {
            log::Level::Debug
        } else {
            log::Level::Info
        };
        meta.level() <= max
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let (secs, millis) = now();
        let msg = redact(&record.args().to_string());
        self.write(&format_line(
            secs,
            millis,
            record.level(),
            record.target(),
            &msg,
        ));
    }

    fn flush(&self) {
        if let Ok(mut slot) = self.file.lock()
            && let Some((_, f)) = slot.as_mut()
        {
            let _ = f.flush();
        }
    }
}

/// Start logging to `<base>/logs`, after deleting old files. Returns the log folder.
pub fn init(base: &Path) -> std::io::Result<PathBuf> {
    let dir = log_dir(base);
    std::fs::create_dir_all(&dir)?;
    let removed = cleanup(&dir, now().0.div_euclid(86_400), KEEP_DAYS);
    let logger = Box::leak(Box::new(FileLogger {
        dir: dir.clone(),
        file: Mutex::new(None),
    }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    if removed > 0 {
        log::info!("deleted {removed} log files older than {KEEP_DAYS} days");
    }
    install_panic_hook();
    Ok(dir)
}

/// Log panics (message, location, backtrace), then run the previous hook.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".into());
        let at = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_default();
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_owned();
        log::error!(
            "panic in thread {thread} at {at}: {msg}\n{}",
            std::backtrace::Backtrace::force_capture()
        );
        log::logger().flush();
        previous(info);
    }));
}

/// `SurfCraft <version> on <os> <arch>`.
pub fn app_line() -> String {
    format!(
        "SurfCraft {} on {} {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/unit-tmp")
            .join(name);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn dates_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        for day in [-1000, 0, 11_016, 11_017, 20_000, 20_759, 60_000] {
            let (y, m, d) = civil_from_days(day);
            assert_eq!(days_from_civil(y, m, d), day);
        }
        // 2026-11-02T14:03:05Z
        let t = days_from_civil(2026, 11, 2) * 86_400 + 14 * 3600 + 3 * 60 + 5;
        assert_eq!(rfc3339(t, 7), "2026-11-02T14:03:05.007Z");
        assert_eq!(
            line_time(&format_line(t, 7, log::Level::Info, "x", "m")),
            Some(t)
        );
        assert_eq!(line_time("    at src/main.rs"), None);
        assert_eq!(line_time("2026-13-02T14:03:05Z"), None);
        assert_eq!(file_day(&file_name(20_000)), Some(20_000));
        assert_eq!(file_day("surfcraft-2026-11-02.log.bak"), None);
        assert_eq!(file_day("other-2026-11-02.log"), None);
    }

    #[test]
    fn time_filter_keeps_recent_lines_and_their_continuations() {
        let t0 = days_from_civil(2026, 11, 2) * 86_400;
        let mut text = String::new();
        text += &format_line(t0, 0, log::Level::Info, "surfcraft", "old");
        text += &format_line(
            t0 + 3600,
            0,
            log::Level::Error,
            "surfcraft",
            "panic\nframe 1\nframe 2",
        );
        text += &format_line(t0 + 7200, 0, log::Level::Info, "surfcraft", "new");
        assert_eq!(filter_since(&text, None), text);
        let recent = filter_since(&text, Some(t0 + 1800));
        assert!(!recent.contains("old"));
        assert!(recent.contains("panic\n    frame 1\n    frame 2\n"));
        assert!(recent.contains("new"));
        assert_eq!(recent.lines().count(), 4);
        assert_eq!(filter_since(&text, Some(t0 + 7201)), "");
    }

    #[test]
    fn collect_reads_files_in_order_and_skips_old_days() {
        let dir = temp("log-collect");
        let day = days_from_civil(2026, 11, 2);
        for (d, msg) in [
            (day - 2, "two days ago"),
            (day, "today"),
            (day - 1, "yesterday"),
        ] {
            let line = format_line(d * 86_400 + 600, 0, log::Level::Info, "surfcraft", msg);
            std::fs::write(dir.join(file_name(d)), line).unwrap();
        }
        std::fs::write(dir.join("notes.txt"), "2026-11-02T00:00:00Z not a log").unwrap();
        let all = collect(&dir, None);
        let order: Vec<&str> = all
            .lines()
            .map(|l| l.rsplit(": ").next().unwrap())
            .collect();
        assert_eq!(order, ["two days ago", "yesterday", "today"]);
        let since = collect(&dir, Some((day - 1) * 86_400));
        assert!(!since.contains("two days ago") && since.contains("yesterday"));
    }

    #[test]
    fn retention_deletes_only_old_log_files() {
        let dir = temp("log-retention");
        let today = days_from_civil(2026, 11, 2);
        for age in [0, 1, 13, 14, 30] {
            std::fs::write(dir.join(file_name(today - age)), "x").unwrap();
        }
        let old_other = dir.join("surfcraft-2020-01-01.txt");
        std::fs::write(&old_other, "keep").unwrap();
        std::fs::create_dir_all(dir.join(file_name(today - 40))).unwrap();
        assert_eq!(cleanup(&dir, today, KEEP_DAYS), 2);
        let left: Vec<i64> = log_files(&dir).iter().map(|(d, _)| today - d).collect();
        assert_eq!(left, [13, 1, 0]);
        assert!(old_other.exists(), "other files are never touched");
        assert!(dir.join(file_name(today - 40)).is_dir());
    }

    #[test]
    fn home_folder_is_redacted() {
        assert_eq!(
            redact_with("/Users/ann/boards/a.brd and /Users/ann", "/Users/ann/"),
            "~/boards/a.brd and ~"
        );
        assert_eq!(redact_with("/x", "/"), "/x");
        assert_eq!(
            redact_with(r"C:\Users\Ann\Desktop\b.brd", r"C:\Users\Ann"),
            r"~\Desktop\b.brd"
        );
    }

    #[test]
    fn logger_writes_daily_files_with_timestamps() {
        let dir = temp("log-writer");
        let logger = FileLogger {
            dir: dir.clone(),
            file: Mutex::new(None),
        };
        logger.write(&format_line(
            now().0,
            0,
            log::Level::Info,
            "surfcraft",
            "hello",
        ));
        let files = log_files(&dir);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, now().0.div_euclid(86_400));
        let text = std::fs::read_to_string(&files[0].1).unwrap();
        assert!(line_time(&text).is_some() && text.ends_with("surfcraft: hello\n"));
    }
}
