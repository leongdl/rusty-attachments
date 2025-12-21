//! Platform-specific machine identifier utilities.

/// Get a unique machine identifier for cache partitioning.
///
/// This identifier is used to partition hash caches so that cached hashes
/// from one machine are not incorrectly used on another machine where
/// the same file path might have different content.
///
/// # Platform Behavior
/// - Linux: Reads `/etc/machine-id`
/// - macOS: Uses IOPlatformUUID from IOKit
/// - Windows: Reads MachineGuid from registry
/// - Fallback: Returns "unknown-machine-id"
///
/// # Returns
/// A string identifier unique to this machine.
pub fn get_machine_id() -> String {
    get_machine_id_impl().unwrap_or_else(|| "unknown-machine-id".to_string())
}

#[cfg(target_os = "linux")]
fn get_machine_id_impl() -> Option<String> {
    use std::io::Read;

    let mut file: std::fs::File = std::fs::File::open("/etc/machine-id").ok()?;
    let mut contents: String = String::new();
    file.read_to_string(&mut contents).ok()?;
    let trimmed: &str = contents.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(target_os = "macos")]
fn get_machine_id_impl() -> Option<String> {
    use std::process::Command;

    // Use ioreg to get the IOPlatformUUID
    let output: std::process::Output = Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout: String = String::from_utf8_lossy(&output.stdout).to_string();

    // Parse the output to find IOPlatformUUID
    for line in stdout.lines() {
        let trimmed: &str = line.trim();
        if trimmed.contains("IOPlatformUUID") {
            // Format: "IOPlatformUUID" = "XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"
            if let Some(start) = trimmed.rfind('"') {
                let before_last: &str = &trimmed[..start];
                if let Some(uuid_start) = before_last.rfind('"') {
                    let uuid: &str = &before_last[uuid_start + 1..];
                    if !uuid.is_empty() {
                        return Some(uuid.to_string());
                    }
                }
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn get_machine_id_impl() -> Option<String> {
    use std::process::Command;

    // Use reg query to get MachineGuid
    let output: std::process::Output = Command::new("reg")
        .args([
            "query",
            r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout: String = String::from_utf8_lossy(&output.stdout).to_string();

    // Parse the output to find MachineGuid value
    for line in stdout.lines() {
        let trimmed: &str = line.trim();
        if trimmed.contains("MachineGuid") {
            // Format: "    MachineGuid    REG_SZ    XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                return Some(parts[2].to_string());
            }
        }
    }

    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn get_machine_id_impl() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_machine_id_returns_string() {
        let id: String = get_machine_id();
        assert!(!id.is_empty());
    }

    #[test]
    fn test_get_machine_id_deterministic() {
        let id1: String = get_machine_id();
        let id2: String = get_machine_id();
        assert_eq!(id1, id2);
    }
}
