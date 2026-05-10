use std::path::{Path, PathBuf};

use anyhow::Context;

pub fn pid_file_path() -> anyhow::Result<PathBuf> {
    Ok(pid_file_path_for_home(&home_dir()?))
}

pub fn app_support_dir_path() -> anyhow::Result<PathBuf> {
    Ok(app_support_dir_path_for_home(&home_dir()?))
}

pub fn launch_agent_plist_path() -> anyhow::Result<PathBuf> {
    Ok(launch_agent_plist_path_for_home(&home_dir()?))
}

pub fn log_file_path() -> anyhow::Result<PathBuf> {
    Ok(log_file_path_for_home(&home_dir()?))
}

pub fn service_metadata_path() -> anyhow::Result<PathBuf> {
    Ok(service_metadata_path_for_home(&home_dir()?))
}

pub fn socket_file_path() -> anyhow::Result<PathBuf> {
    Ok(socket_file_path_for_home(&home_dir()?))
}

pub fn codex_desktop_source_state_path() -> anyhow::Result<PathBuf> {
    Ok(codex_desktop_source_state_path_for_home(&home_dir()?))
}

pub fn codex_sessions_dir_path() -> anyhow::Result<PathBuf> {
    Ok(codex_sessions_dir_path_for_home(&home_dir()?))
}

pub fn codex_session_index_path() -> anyhow::Result<PathBuf> {
    Ok(codex_session_index_path_for_home(&home_dir()?))
}

pub fn default_config_file_path() -> anyhow::Result<PathBuf> {
    Ok(default_config_file_path_for_home(&home_dir()?))
}

pub fn pid_file_path_for_home(home: &Path) -> PathBuf {
    app_support_dir_path_for_home(home).join("agents-notifier.pid")
}

pub fn app_support_dir_path_for_home(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Application Support")
        .join("agents-notifier")
}

pub fn launch_agent_plist_path_for_home(home: &Path) -> PathBuf {
    home.join("Library")
        .join("LaunchAgents")
        .join("com.agents-notifier.service.plist")
}

pub fn log_file_path_for_home(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Logs")
        .join("agents-notifier")
        .join("agents-notifier.log")
}

pub fn socket_file_path_for_home(home: &Path) -> PathBuf {
    app_support_dir_path_for_home(home).join("agents-notifier.sock")
}

pub fn service_metadata_path_for_home(home: &Path) -> PathBuf {
    app_support_dir_path_for_home(home).join("service.json")
}

pub fn codex_desktop_source_state_path_for_home(home: &Path) -> PathBuf {
    app_support_dir_path_for_home(home).join("codex-desktop-source-state.json")
}

pub fn codex_sessions_dir_path_for_home(home: &Path) -> PathBuf {
    home.join(".codex").join("sessions")
}

pub fn codex_session_index_path_for_home(home: &Path) -> PathBuf {
    home.join(".codex").join("session_index.jsonl")
}

pub fn default_config_file_path_for_home(home: &Path) -> PathBuf {
    home.join(".config")
        .join("agents-notifier")
        .join("config.toml")
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
    fn builds_macos_app_support_dir_path() {
        let path = app_support_dir_path_for_home(Path::new("/Users/tester"));

        assert_eq!(
            path,
            Path::new("/Users/tester")
                .join("Library")
                .join("Application Support")
                .join("agents-notifier")
        );
    }

    #[test]
    fn builds_macos_launch_agent_plist_path() {
        let path = launch_agent_plist_path_for_home(Path::new("/Users/tester"));

        assert_eq!(
            path,
            Path::new("/Users/tester")
                .join("Library")
                .join("LaunchAgents")
                .join("com.agents-notifier.service.plist")
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

    #[test]
    fn builds_macos_socket_file_path() {
        let path = socket_file_path_for_home(Path::new("/Users/tester"));

        assert_eq!(
            path,
            Path::new("/Users/tester")
                .join("Library")
                .join("Application Support")
                .join("agents-notifier")
                .join("agents-notifier.sock")
        );
    }

    #[test]
    fn builds_service_metadata_path() {
        let path = service_metadata_path_for_home(Path::new("/Users/tester"));

        assert_eq!(
            path,
            Path::new("/Users/tester")
                .join("Library")
                .join("Application Support")
                .join("agents-notifier")
                .join("service.json")
        );
    }

    #[test]
    fn builds_codex_desktop_source_state_path() {
        let path = codex_desktop_source_state_path_for_home(Path::new("/Users/tester"));

        assert_eq!(
            path,
            Path::new("/Users/tester")
                .join("Library")
                .join("Application Support")
                .join("agents-notifier")
                .join("codex-desktop-source-state.json")
        );
    }

    #[test]
    fn builds_codex_sessions_dir_path() {
        let path = codex_sessions_dir_path_for_home(Path::new("/Users/tester"));

        assert_eq!(
            path,
            Path::new("/Users/tester").join(".codex").join("sessions")
        );
    }

    #[test]
    fn builds_codex_session_index_path() {
        let path = codex_session_index_path_for_home(Path::new("/Users/tester"));

        assert_eq!(
            path,
            Path::new("/Users/tester")
                .join(".codex")
                .join("session_index.jsonl")
        );
    }

    #[test]
    fn builds_default_config_file_path() {
        let path = default_config_file_path_for_home(Path::new("/Users/tester"));

        assert_eq!(
            path,
            Path::new("/Users/tester")
                .join(".config")
                .join("agents-notifier")
                .join("config.toml")
        );
    }
}
