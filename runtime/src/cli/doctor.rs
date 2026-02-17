//! Environment readiness check.

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

/// Check Chromium availability, socket path, and available memory.
pub async fn run() -> Result<()> {
    println!("Cortex Doctor");
    println!("=============");
    println!();

    // OS and architecture
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    println!("OS:   {os}");
    println!("Arch: {arch}");
    println!();

    // Check Chromium
    let chromium_path = find_chromium();
    match &chromium_path {
        Some(path) => println!("[OK] Chromium found: {}", path.display()),
        None => println!("[!!] Chromium NOT found. Run `cortex install` to download Chrome for Testing."),
    }

    // Check socket path
    let socket_path = PathBuf::from("/tmp/cortex.sock");
    let socket_dir = socket_path.parent().unwrap_or(&socket_path);
    if socket_dir.exists() {
        println!("[OK] Socket path /tmp/cortex.sock is writable");
    } else {
        println!("[!!] Socket directory does not exist: {}", socket_dir.display());
    }

    // Check available memory
    let mem_mb = get_available_memory_mb();
    match mem_mb {
        Some(mb) => {
            if mb >= 256 {
                println!("[OK] Available memory: {mb}MB (>= 256MB required)");
            } else {
                println!("[!!] Available memory: {mb}MB (< 256MB — may be insufficient)");
            }
        }
        None => println!("[??] Could not determine available memory"),
    }

    println!();
    let ready = chromium_path.is_some();
    if ready {
        println!("Status: READY");
    } else {
        println!("Status: NOT READY");
        println!("  Run `cortex install` to set up Chromium.");
    }

    Ok(())
}

/// Find Chromium binary by checking multiple locations.
fn find_chromium() -> Option<PathBuf> {
    // 1. Check CORTEX_CHROMIUM_PATH env
    if let Ok(p) = std::env::var("CORTEX_CHROMIUM_PATH") {
        let path = PathBuf::from(&p);
        if path.exists() {
            return Some(path);
        }
    }

    // 2. Check ~/.cortex/chromium/
    if let Some(home) = dirs::home_dir() {
        let candidates = if cfg!(target_os = "macos") {
            vec![
                home.join(".cortex/chromium/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"),
                home.join(".cortex/chromium/chrome"),
            ]
        } else {
            vec![
                home.join(".cortex/chromium/chrome"),
                home.join(".cortex/chromium/chrome-linux64/chrome"),
            ]
        };
        for c in candidates {
            if c.exists() {
                return Some(c);
            }
        }
    }

    // 3. Check system PATH
    if let Ok(output) = Command::new("which").arg("google-chrome").output() {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path_str.is_empty() {
                return Some(PathBuf::from(path_str));
            }
        }
    }
    if let Ok(output) = Command::new("which").arg("chromium").output() {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path_str.is_empty() {
                return Some(PathBuf::from(path_str));
            }
        }
    }

    // 4. Common macOS locations
    if cfg!(target_os = "macos") {
        let common = PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
        if common.exists() {
            return Some(common);
        }
    }

    None
}

/// Get available memory in MB (platform-specific).
fn get_available_memory_mb() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&output.stdout);
        let bytes: u64 = s.trim().parse().ok()?;
        Some(bytes / 1_048_576)
    }
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("free")
            .args(["-m"])
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&output.stdout);
        for line in s.lines() {
            if line.starts_with("Mem:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 7 {
                    return parts[6].parse().ok();
                }
            }
        }
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}
