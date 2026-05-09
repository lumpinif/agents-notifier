use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use chrono::Utc;
use serde::{Deserialize, Serialize};

pub const LAUNCH_AGENT_LABEL: &str = "com.agents-notifier.service";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceDefinition {
    pub binary_path: PathBuf,
    pub config_path: PathBuf,
    pub working_dir: PathBuf,
    pub log_file: PathBuf,
    pub home: String,
    pub env_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceStartOutcome {
    AlreadyRunning,
    Started,
    UpdatedAndStarted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceStopOutcome {
    NotInstalled,
    AlreadyStopped,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatus {
    pub installed: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub platform: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceMetadata {
    pub binary_path: String,
    pub config_path: String,
    pub log_file: String,
    pub installed_at: String,
}

pub trait LaunchctlRunner {
    fn run(&self, args: &[String]) -> anyhow::Result<String>;
}

pub struct SystemLaunchctl;

impl LaunchctlRunner for SystemLaunchctl {
    fn run(&self, args: &[String]) -> anyhow::Result<String> {
        ensure_macos()?;

        let output = Command::new("launchctl")
            .args(args)
            .output()
            .with_context(|| format!("failed to run launchctl {}", args.join(" ")))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr).trim().to_string();

        if output.status.success() {
            Ok(combined)
        } else {
            anyhow::bail!("launchctl {} failed: {}", args.join(" "), combined);
        }
    }
}

pub struct LaunchAgentManager<R> {
    runner: R,
    uid: u32,
}

impl LaunchAgentManager<SystemLaunchctl> {
    pub fn system() -> anyhow::Result<Self> {
        ensure_macos()?;
        Ok(Self {
            runner: SystemLaunchctl,
            uid: current_uid()?,
        })
    }
}

impl<R: LaunchctlRunner> LaunchAgentManager<R> {
    pub fn with_uid(runner: R, uid: u32) -> Self {
        Self { runner, uid }
    }

    pub fn install_or_update(
        &self,
        definition: &ServiceDefinition,
        plist_path: &Path,
        metadata_path: &Path,
    ) -> anyhow::Result<ServiceStartOutcome> {
        if let Some(parent) = definition.log_file.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create log directory `{}`", parent.display())
            })?;
        }

        let desired_plist = build_plist(definition);
        let installed = plist_path.exists();
        let current_plist = if installed {
            Some(fs::read_to_string(plist_path).with_context(|| {
                format!(
                    "failed to read LaunchAgent plist `{}`",
                    plist_path.display()
                )
            })?)
        } else {
            None
        };
        let needs_update = current_plist.as_deref() != Some(desired_plist.as_str());
        let status = self.status(plist_path)?;

        if installed && !needs_update && status.running {
            save_metadata(metadata_path, definition)?;
            return Ok(ServiceStartOutcome::AlreadyRunning);
        }

        if needs_update {
            self.bootout_targets();
            write_plist(plist_path, &desired_plist)?;
        } else if !installed {
            write_plist(plist_path, &desired_plist)?;
        }

        let domain = self.preferred_domain();
        let target = launchd_target(&domain);
        let plist = plist_path.display().to_string();
        let bootstrap_args = vec!["bootstrap".to_string(), domain, plist];
        if self.runner.run(&bootstrap_args).is_err() {
            let kickstart_args = vec!["kickstart".to_string(), "-kp".to_string(), target.clone()];
            self.runner.run(&kickstart_args)?;
        } else {
            let kickstart_args = vec!["kickstart".to_string(), "-kp".to_string(), target];
            self.runner.run(&kickstart_args)?;
        }

        save_metadata(metadata_path, definition)?;

        if installed && needs_update {
            Ok(ServiceStartOutcome::UpdatedAndStarted)
        } else {
            Ok(ServiceStartOutcome::Started)
        }
    }

    pub fn stop(&self, plist_path: &Path) -> anyhow::Result<ServiceStopOutcome> {
        let status = self.status(plist_path)?;
        if !status.installed {
            return Ok(ServiceStopOutcome::NotInstalled);
        }

        self.bootout_targets();

        if status.running {
            Ok(ServiceStopOutcome::Stopped)
        } else {
            Ok(ServiceStopOutcome::AlreadyStopped)
        }
    }

    pub fn status(&self, plist_path: &Path) -> anyhow::Result<ServiceStatus> {
        let mut status = ServiceStatus {
            installed: plist_path.exists(),
            running: false,
            pid: None,
            platform: "launchd",
        };

        if !status.installed {
            return Ok(status);
        }

        for domain in self.launchd_domains() {
            let target = launchd_target(&domain);
            let args = vec!["print".to_string(), target];
            if let Ok(output) = self.runner.run(&args) {
                let parsed = parse_launchctl_print(&output);
                status.running = parsed.running;
                status.pid = parsed.pid;
                return Ok(status);
            }
        }

        Ok(status)
    }

    fn preferred_domain(&self) -> String {
        let gui_domain = self.gui_domain();
        let args = vec!["print".to_string(), gui_domain.clone()];
        if self.runner.run(&args).is_ok() {
            gui_domain
        } else {
            self.user_domain()
        }
    }

    fn launchd_domains(&self) -> Vec<String> {
        let preferred = self.preferred_domain();
        let gui_domain = self.gui_domain();
        let user_domain = self.user_domain();
        if preferred == gui_domain {
            vec![gui_domain, user_domain]
        } else {
            vec![user_domain, gui_domain]
        }
    }

    fn bootout_targets(&self) {
        for domain in self.launchd_domains() {
            let args = vec!["bootout".to_string(), launchd_target(&domain)];
            let _ = self.runner.run(&args);
        }
    }

    fn gui_domain(&self) -> String {
        format!("gui/{}", self.uid)
    }

    fn user_domain(&self) -> String {
        format!("user/{}", self.uid)
    }
}

pub fn build_plist(definition: &ServiceDefinition) -> String {
    let binary = xml_escape(&definition.binary_path.display().to_string());
    let config = xml_escape(&definition.config_path.display().to_string());
    let working_dir = xml_escape(&definition.working_dir.display().to_string());
    let log_file = xml_escape(&definition.log_file.display().to_string());
    let home = xml_escape(&definition.home);
    let env_path = xml_escape(&definition.env_path);

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{label}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{binary}</string>
		<string>watch</string>
		<string>--config</string>
		<string>{config}</string>
	</array>
	<key>WorkingDirectory</key>
	<string>{working_dir}</string>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<true/>
	<key>LimitLoadToSessionType</key>
	<array>
		<string>Aqua</string>
		<string>Background</string>
	</array>
	<key>EnvironmentVariables</key>
	<dict>
		<key>HOME</key>
		<string>{home}</string>
		<key>PATH</key>
		<string>{env_path}</string>
	</dict>
	<key>StandardOutPath</key>
	<string>{log_file}</string>
	<key>StandardErrorPath</key>
	<string>{log_file}</string>
</dict>
</plist>
"#,
        label = LAUNCH_AGENT_LABEL,
        binary = binary,
        config = config,
        working_dir = working_dir,
        log_file = log_file,
        home = home,
        env_path = env_path,
    )
}

pub fn load_metadata(path: &Path) -> anyhow::Result<ServiceMetadata> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read service metadata `{}`", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse service metadata `{}`", path.display()))
}

pub fn save_metadata(path: &Path, definition: &ServiceDefinition) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create service metadata directory `{}`",
                parent.display()
            )
        })?;
    }

    let metadata = ServiceMetadata {
        binary_path: definition.binary_path.display().to_string(),
        config_path: definition.config_path.display().to_string(),
        log_file: definition.log_file.display().to_string(),
        installed_at: Utc::now().to_rfc3339(),
    };
    let raw = serde_json::to_string_pretty(&metadata).context("failed to serialize metadata")?;
    fs::write(path, raw)
        .with_context(|| format!("failed to write service metadata `{}`", path.display()))
}

fn write_plist(path: &Path, plist: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create LaunchAgent directory `{}`",
                parent.display()
            )
        })?;
    }
    fs::write(path, plist)
        .with_context(|| format!("failed to write LaunchAgent plist `{}`", path.display()))
}

struct ParsedLaunchctlPrint {
    running: bool,
    pid: Option<u32>,
}

fn parse_launchctl_print(output: &str) -> ParsedLaunchctlPrint {
    let mut parsed = ParsedLaunchctlPrint {
        running: false,
        pid: None,
    };

    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(raw_pid) = trimmed.strip_prefix("pid = ")
            && let Ok(pid) = raw_pid.parse::<u32>()
            && pid > 0
        {
            parsed.pid = Some(pid);
            parsed.running = true;
        }
        if trimmed.contains("state = running") {
            parsed.running = true;
        }
    }

    parsed
}

fn launchd_target(domain: &str) -> String {
    format!("{domain}/{LAUNCH_AGENT_LABEL}")
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn ensure_macos() -> anyhow::Result<()> {
    if cfg!(target_os = "macos") {
        Ok(())
    } else {
        anyhow::bail!("LaunchAgent service management is only supported on macOS")
    }
}

fn current_uid() -> anyhow::Result<u32> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("failed to detect current user id")?;
    if !output.status.success() {
        anyhow::bail!("failed to detect current user id");
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    raw.trim()
        .parse::<u32>()
        .context("failed to parse current user id")
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use anyhow::anyhow;
    use tempfile::tempdir;

    use super::*;

    struct FakeLaunchctl {
        commands: RefCell<Vec<Vec<String>>>,
        target_print: Option<String>,
        gui_available: bool,
    }

    impl FakeLaunchctl {
        fn new() -> Self {
            Self {
                commands: RefCell::new(Vec::new()),
                target_print: None,
                gui_available: true,
            }
        }

        fn with_target_print(output: &str) -> Self {
            Self {
                commands: RefCell::new(Vec::new()),
                target_print: Some(output.to_string()),
                gui_available: true,
            }
        }
    }

    impl LaunchctlRunner for FakeLaunchctl {
        fn run(&self, args: &[String]) -> anyhow::Result<String> {
            self.commands.borrow_mut().push(args.to_vec());
            if args == ["print", "gui/501"] {
                if self.gui_available {
                    return Ok("domain = gui/501".to_string());
                }
                return Err(anyhow!("gui unavailable"));
            }
            if args == ["print", "user/501"] {
                return Ok("domain = user/501".to_string());
            }
            if args == ["print", "gui/501/com.agents-notifier.service"]
                || args == ["print", "user/501/com.agents-notifier.service"]
            {
                return self
                    .target_print
                    .clone()
                    .ok_or_else(|| anyhow!("target not loaded"));
            }
            Ok(String::new())
        }
    }

    #[test]
    fn plist_contains_launch_agent_worker_command() {
        let definition = test_definition();

        let plist = build_plist(&definition);

        assert!(plist.contains("<string>com.agents-notifier.service</string>"));
        assert!(plist.contains("<string>/bin/agents-notifier</string>"));
        assert!(plist.contains("<string>watch</string>"));
        assert!(plist.contains("<string>--config</string>"));
        assert!(plist.contains("<string>/tmp/config.toml</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("<string>/tmp/agents-notifier.log</string>"));
        assert!(plist.contains("<string>/Users/tester</string>"));
        assert!(plist.contains("<string>/usr/local/bin:/usr/bin</string>"));
    }

    #[test]
    fn plist_escapes_xml_values() {
        let definition = ServiceDefinition {
            binary_path: PathBuf::from("/tmp/a&b/agents-notifier"),
            config_path: PathBuf::from("/tmp/config<test>.toml"),
            working_dir: PathBuf::from("/tmp/work\"dir"),
            log_file: PathBuf::from("/tmp/log'file.log"),
            home: "/Users/a&b".to_string(),
            env_path: "/bin:/tmp/a&b".to_string(),
        };

        let plist = build_plist(&definition);

        assert!(plist.contains("/tmp/a&amp;b/agents-notifier"));
        assert!(plist.contains("/tmp/config&lt;test&gt;.toml"));
        assert!(plist.contains("/tmp/work&quot;dir"));
        assert!(plist.contains("/tmp/log&apos;file.log"));
        assert!(plist.contains("/Users/a&amp;b"));
        assert!(plist.contains("/bin:/tmp/a&amp;b"));
    }

    #[test]
    fn saves_and_loads_metadata() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("service.json");

        save_metadata(&path, &test_definition()).expect("metadata should be saved");
        let metadata = load_metadata(&path).expect("metadata should load");

        assert_eq!(metadata.binary_path, "/bin/agents-notifier");
        assert_eq!(metadata.config_path, "/tmp/config.toml");
        assert_eq!(metadata.log_file, "/tmp/agents-notifier.log");
        assert!(!metadata.installed_at.is_empty());
    }

    #[test]
    fn install_writes_plist_and_bootstraps_preferred_domain() {
        let dir = tempdir().expect("tempdir should be created");
        let plist = dir.path().join("LaunchAgents").join("service.plist");
        let metadata = dir.path().join("Application Support").join("service.json");
        let runner = FakeLaunchctl::new();
        let manager = LaunchAgentManager::with_uid(runner, 501);

        let outcome = manager
            .install_or_update(&test_definition(), &plist, &metadata)
            .expect("service should start");

        assert_eq!(outcome, ServiceStartOutcome::Started);
        assert!(plist.exists());
        assert!(metadata.exists());
        assert!(
            fs::read_to_string(&plist)
                .expect("plist should be readable")
                .contains("<string>watch</string>")
        );
        let commands = manager.runner.commands.borrow();
        assert!(commands.contains(&vec![
            "bootstrap".to_string(),
            "gui/501".to_string(),
            plist.display().to_string()
        ]));
        assert!(commands.contains(&vec![
            "kickstart".to_string(),
            "-kp".to_string(),
            "gui/501/com.agents-notifier.service".to_string()
        ]));
    }

    #[test]
    fn already_running_service_is_not_restarted_when_plist_matches() {
        let dir = tempdir().expect("tempdir should be created");
        let plist = dir.path().join("service.plist");
        let metadata = dir.path().join("service.json");
        fs::write(&plist, build_plist(&test_definition())).expect("plist should be written");
        let runner = FakeLaunchctl::with_target_print("state = running\npid = 42\n");
        let manager = LaunchAgentManager::with_uid(runner, 501);

        let outcome = manager
            .install_or_update(&test_definition(), &plist, &metadata)
            .expect("service should be inspected");

        assert_eq!(outcome, ServiceStartOutcome::AlreadyRunning);
        let commands = manager.runner.commands.borrow();
        assert!(!commands.iter().any(|command| command[0] == "bootstrap"));
        assert!(!commands.iter().any(|command| command[0] == "kickstart"));
    }

    #[test]
    fn status_parses_running_pid() {
        let dir = tempdir().expect("tempdir should be created");
        let plist = dir.path().join("service.plist");
        fs::write(&plist, "plist").expect("plist should be written");
        let runner = FakeLaunchctl::with_target_print("state = running\npid = 123\n");
        let manager = LaunchAgentManager::with_uid(runner, 501);

        let status = manager.status(&plist).expect("status should load");

        assert!(status.installed);
        assert!(status.running);
        assert_eq!(status.pid, Some(123));
    }

    #[test]
    fn stop_boots_out_loaded_targets() {
        let dir = tempdir().expect("tempdir should be created");
        let plist = dir.path().join("service.plist");
        fs::write(&plist, "plist").expect("plist should be written");
        let runner = FakeLaunchctl::with_target_print("state = running\npid = 123\n");
        let manager = LaunchAgentManager::with_uid(runner, 501);

        let outcome = manager.stop(&plist).expect("service should stop");

        assert_eq!(outcome, ServiceStopOutcome::Stopped);
        let commands = manager.runner.commands.borrow();
        assert!(commands.contains(&vec![
            "bootout".to_string(),
            "gui/501/com.agents-notifier.service".to_string()
        ]));
        assert!(commands.contains(&vec![
            "bootout".to_string(),
            "user/501/com.agents-notifier.service".to_string()
        ]));
    }

    fn test_definition() -> ServiceDefinition {
        ServiceDefinition {
            binary_path: PathBuf::from("/bin/agents-notifier"),
            config_path: PathBuf::from("/tmp/config.toml"),
            working_dir: PathBuf::from("/tmp"),
            log_file: PathBuf::from("/tmp/agents-notifier.log"),
            home: "/Users/tester".to_string(),
            env_path: "/usr/local/bin:/usr/bin".to_string(),
        }
    }
}
