use anyhow::Result;
use std::path::Path;
use std::process::Command;

pub fn check_blackhole_installed() -> bool {
    Path::new("/Library/Audio/Plug-Ins/HAL/BlackHole2ch.driver").exists()
}

pub fn remove_quarantine(path: &str) -> Result<()> {
    let status = Command::new("xattr")
        .args(["-d", "com.apple.quarantine", path])
        .status()?;
    if !status.success() {
        // Not an error if the attribute doesn't exist
    }
    Ok(())
}

pub fn install_launch_agent() -> Result<()> {
    let home = std::env::var("HOME")?;
    let plist_dir = format!("{}/Library/LaunchAgents", home);
    std::fs::create_dir_all(&plist_dir)?;

    let binary_path = std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("/usr/local/bin/wisprnito"));

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.wisprnito</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <false/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>"#,
        binary_path.display()
    );

    let plist_path = format!("{}/com.wisprnito.plist", plist_dir);
    std::fs::write(&plist_path, plist_content)?;
    println!("Launch agent installed at {}", plist_path);
    Ok(())
}
