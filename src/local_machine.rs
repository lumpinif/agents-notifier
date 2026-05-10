use std::process::Command;

use anyhow::{Context, anyhow};

pub fn computer_name() -> anyhow::Result<String> {
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

    parse_computer_name(&output.stdout)
}

fn parse_computer_name(raw: &[u8]) -> anyhow::Result<String> {
    let value = std::str::from_utf8(raw)
        .context("macOS computer name was not valid UTF-8")?
        .trim();
    if value.is_empty() {
        return Err(anyhow!("macOS computer name is empty"));
    }

    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_computer_name_output() {
        assert_eq!(
            parse_computer_name(" Felix's MacBook Pro\n".as_bytes()).unwrap(),
            "Felix's MacBook Pro"
        );
    }

    #[test]
    fn rejects_empty_computer_name() {
        assert!(parse_computer_name("\n".as_bytes()).is_err());
    }
}
