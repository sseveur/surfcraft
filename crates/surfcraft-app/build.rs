//! Embed the icon and version info in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=../../packaging/windows/surfcraft.ico");
    // `winresource` is only a build dependency on Windows hosts; also check the target so a Windows host building
    // for another OS doesn't try to embed a Windows resource.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../packaging/windows/surfcraft.ico");
        res.set("ProductName", "SurfCraft");
        res.set("FileDescription", "SurfCraft surfboard design");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed Windows icon: {e}");
        }
    }
}
