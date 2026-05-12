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
