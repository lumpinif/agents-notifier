#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;

use anyhow::{Context, anyhow};

pub fn computer_name() -> anyhow::Result<String> {
    #[cfg(target_os = "macos")]
    return macos_computer_name();

    #[cfg(target_os = "linux")]
    return linux_computer_name();

    #[cfg(windows)]
    return windows_computer_name();
}

#[cfg(target_os = "macos")]
fn macos_computer_name() -> anyhow::Result<String> {
    let output = Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
        .context("failed to read macOS computer name with scutil")?;

    if !output.status.success() {
        return Err(anyhow!(
            "failed to read macOS computer name: scutil exited with status {}",
            output.status
        ));
    }

    parse_machine_name(&output.stdout, "macOS computer name")
}

#[cfg(target_os = "linux")]
fn linux_computer_name() -> anyhow::Result<String> {
    let output = Command::new("hostname")
        .output()
        .context("failed to read Linux machine name with hostname")?;

    if !output.status.success() {
        return Err(anyhow!(
            "failed to read Linux machine name: hostname exited with status {}",
            output.status
        ));
    }

    parse_machine_name(&output.stdout, "Linux machine name")
}

#[cfg(windows)]
fn windows_computer_name() -> anyhow::Result<String> {
    let value = std::env::var("COMPUTERNAME").context("COMPUTERNAME is not set")?;
    parse_machine_name(value.as_bytes(), "Windows computer name")
}

fn parse_machine_name(raw: &[u8], label: &str) -> anyhow::Result<String> {
    let value = std::str::from_utf8(raw)
        .with_context(|| format!("{label} was not valid UTF-8"))?
        .trim();
    if value.is_empty() {
        return Err(anyhow!("{label} is empty"));
    }

    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_computer_name_output() {
        assert_eq!(
            parse_machine_name(" Felix's MacBook Pro\n".as_bytes(), "machine name").unwrap(),
            "Felix's MacBook Pro"
        );
    }

    #[test]
    fn rejects_empty_computer_name() {
        assert!(parse_machine_name("\n".as_bytes(), "machine name").is_err());
    }
}
