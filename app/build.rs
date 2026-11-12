fn main() {
    slint_build::compile("ui/app.slint").expect("Slint UI failed to compile");

    // On Windows, embed the version resource (shown in the file's Properties and read by
    // MoneyKeep Launcher) and, when the repository has one, the app icon assets/icon.ico
    // (Explorer, the taskbar, Alt-Tab, the shortcuts). As in CalcKeep's build.rs.
    #[cfg(windows)]
    {
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("assets")
                .join("icon.ico");
            println!("cargo:rerun-if-changed={}", icon.display());
            let mut res = winresource::WindowsResource::new();
            if icon.is_file() {
                res.set_icon(&icon.display().to_string());
            }
            res.set("ProductName", "GroupKeep");
            res.set("FileDescription", "GroupKeep: divide group expenses");
            res.set("CompanyName", "MoneyKeep Project");
            // Windows wants four numeric parts: 0.1.0-alpha.1 -> 0.1.0.1 (the pre-release
            // number goes last; a plain release gets .0). Each part is 16 bits.
            let part = |name: &str| -> u64 {
                std::env::var(name)
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0)
                    .min(0xffff)
            };
            let build = std::env::var("CARGO_PKG_VERSION_PRE")
                .ok()
                .and_then(|pre| pre.rsplit('.').next().and_then(|n| n.parse::<u64>().ok()))
                .filter(|n| *n <= 0xffff)
                .unwrap_or(0);
            let (major, minor, patch) = (
                part("CARGO_PKG_VERSION_MAJOR"),
                part("CARGO_PKG_VERSION_MINOR"),
                part("CARGO_PKG_VERSION_PATCH"),
            );
            let four = format!("{major}.{minor}.{patch}.{build}");
            let packed = (major << 48) | (minor << 32) | (patch << 16) | build;
            res.set_version_info(winresource::VersionInfo::FILEVERSION, packed);
            res.set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed);
            res.set("FileVersion", &four);
            res.set(
                "ProductVersion",
                &std::env::var("CARGO_PKG_VERSION").unwrap_or_default(),
            );
            if let Err(e) = res.compile() {
                println!("cargo:warning=could not embed the Windows resources: {e}");
            }
        }
    }
}
