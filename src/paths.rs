use std::path::{Path, PathBuf};

use anyhow::Context;

pub fn pid_file_path() -> anyhow::Result<PathBuf> {
    Ok(pid_file_path_for_home(&home_dir()?))
}

pub fn log_file_path() -> anyhow::Result<PathBuf> {
    Ok(log_file_path_for_home(&home_dir()?))
}

pub fn pid_file_path_for_home(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Application Support")
        .join("agents-notifier")
        .join("agents-notifier.pid")
}

pub fn log_file_path_for_home(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Logs")
        .join("agents-notifier")
        .join("agents-notifier.log")
}

fn home_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    if home.is_empty() {
        anyhow::bail!("HOME is empty");
    }

    Ok(PathBuf::from(home))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_macos_pid_file_path() {
        let path = pid_file_path_for_home(Path::new("/Users/tester"));

        assert_eq!(
            path,
            Path::new("/Users/tester")
                .join("Library")
                .join("Application Support")
                .join("agents-notifier")
                .join("agents-notifier.pid")
        );
    }

    #[test]
    fn builds_macos_log_file_path() {
        let path = log_file_path_for_home(Path::new("/Users/tester"));

        assert_eq!(
            path,
            Path::new("/Users/tester")
                .join("Library")
                .join("Logs")
                .join("agents-notifier")
                .join("agents-notifier.log")
        );
    }
}
