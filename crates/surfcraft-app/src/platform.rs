//! Hand a folder, file or URL to the operating system (Finder / Explorer / file manager, browser, mail client).
//! Tests record the request instead of starting anything.

use std::path::Path;

#[cfg(test)]
thread_local! {
    static OPENED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// What tests asked to open on this thread (`open:<path>`, `reveal:<path>`, `url:<url>`), cleared by the call.
#[cfg(test)]
pub fn take_opened() -> Vec<String> {
    OPENED.with(|o| std::mem::take(&mut *o.borrow_mut()))
}

#[cfg(not(test))]
fn spawn(program: &str, args: &[&std::ffi::OsStr]) -> std::io::Result<()> {
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(drop)
}

/// Open a folder in the file manager.
pub fn open_folder(dir: &Path) -> std::io::Result<()> {
    log::info!("open folder {}", crate::logging::path(dir));
    #[cfg(test)]
    {
        OPENED.with(|o| o.borrow_mut().push(format!("open:{}", dir.display())));
        Ok(())
    }
    #[cfg(not(test))]
    {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(windows) {
            "explorer"
        } else {
            "xdg-open"
        };
        spawn(program, &[dir.as_os_str()])
    }
}

/// Show a file selected in the file manager (Linux: open its folder).
pub fn reveal(file: &Path) -> std::io::Result<()> {
    log::info!("reveal {}", crate::logging::path(file));
    #[cfg(test)]
    {
        OPENED.with(|o| o.borrow_mut().push(format!("reveal:{}", file.display())));
        Ok(())
    }
    #[cfg(not(test))]
    {
        if cfg!(target_os = "macos") {
            spawn("open", &["-R".as_ref(), file.as_os_str()])
        } else if cfg!(windows) {
            let mut arg = std::ffi::OsString::from("/select,");
            arg.push(file.as_os_str());
            spawn("explorer", &[arg.as_os_str()])
        } else {
            let dir = file.parent().unwrap_or(Path::new("."));
            spawn("xdg-open", &[dir.as_os_str()])
        }
    }
}

/// Open an `https:` or `mailto:` URL with the default browser or mail client.
pub fn open_url(url: &str) -> std::io::Result<()> {
    // The URL may hold the user's description: log only where it goes.
    log::info!("open {}", url.split('?').next().unwrap_or_default());
    #[cfg(test)]
    {
        OPENED.with(|o| o.borrow_mut().push(format!("url:{url}")));
        Ok(())
    }
    #[cfg(not(test))]
    {
        if cfg!(target_os = "macos") {
            spawn("open", &[url.as_ref()])
        } else if cfg!(windows) {
            // Unlike `cmd /c start`, this passes `&` in the URL through untouched.
            spawn(
                "rundll32",
                &["url.dll,FileProtocolHandler".as_ref(), url.as_ref()],
            )
        } else {
            spawn("xdg-open", &[url.as_ref()])
        }
    }
}
