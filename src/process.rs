use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, anyhow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopOutcome {
    MissingPidFile,
    Stopped { pid: u32 },
}

pub trait ProcessManager {
    fn is_agents_notifier(&self, pid: u32) -> anyhow::Result<bool>;
    fn terminate(&self, pid: u32) -> anyhow::Result<()>;
}

pub struct SystemProcessManager;

impl ProcessManager for SystemProcessManager {
    fn is_agents_notifier(&self, pid: u32) -> anyhow::Result<bool> {
        let output = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .with_context(|| format!("failed to inspect process `{pid}`"))?;

        if !output.status.success() {
            return Ok(false);
        }

        let command = String::from_utf8_lossy(&output.stdout);
        let name = Path::new(command.trim())
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        Ok(name == "agents-notifier")
    }

    fn terminate(&self, pid: u32) -> anyhow::Result<()> {
        let status = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .with_context(|| format!("failed to terminate process `{pid}`"))?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("failed to terminate process `{pid}`"))
        }
    }
}

pub fn stop_with_manager(
    pid_file_path: &Path,
    manager: &dyn ProcessManager,
) -> anyhow::Result<StopOutcome> {
    if !pid_file_path.exists() {
        return Ok(StopOutcome::MissingPidFile);
    }

    let raw_pid = fs::read_to_string(pid_file_path)
        .with_context(|| format!("failed to read pid file `{}`", pid_file_path.display()))?;
    let pid: u32 = raw_pid
        .trim()
        .parse()
        .with_context(|| format!("pid file `{}` is invalid", pid_file_path.display()))?;

    if !manager.is_agents_notifier(pid)? {
        return Err(anyhow!(
            "pid file points to `{pid}`, but it is not an agents-notifier process"
        ));
    }

    manager.terminate(pid)?;
    fs::remove_file(pid_file_path)
        .with_context(|| format!("failed to remove pid file `{}`", pid_file_path.display()))?;

    Ok(StopOutcome::Stopped { pid })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use tempfile::tempdir;

    use super::*;

    struct FakeProcessManager {
        is_agents_notifier: bool,
        terminated: RefCell<Vec<u32>>,
    }

    impl FakeProcessManager {
        fn new(is_agents_notifier: bool) -> Self {
            Self {
                is_agents_notifier,
                terminated: RefCell::new(Vec::new()),
            }
        }
    }

    impl ProcessManager for FakeProcessManager {
        fn is_agents_notifier(&self, _pid: u32) -> anyhow::Result<bool> {
            Ok(self.is_agents_notifier)
        }

        fn terminate(&self, pid: u32) -> anyhow::Result<()> {
            self.terminated.borrow_mut().push(pid);
            Ok(())
        }
    }

    #[test]
    fn missing_pid_file_is_success() {
        let dir = tempdir().expect("tempdir should be created");
        let manager = FakeProcessManager::new(true);

        let outcome = stop_with_manager(&dir.path().join("missing.pid"), &manager)
            .expect("missing pid file should be successful");

        assert_eq!(outcome, StopOutcome::MissingPidFile);
        assert!(manager.terminated.borrow().is_empty());
    }

    #[test]
    fn stops_agents_notifier_process_and_removes_pid_file() {
        let dir = tempdir().expect("tempdir should be created");
        let pid_file = dir.path().join("agents-notifier.pid");
        fs::write(&pid_file, "123").expect("pid file should be written");
        let manager = FakeProcessManager::new(true);

        let outcome =
            stop_with_manager(&pid_file, &manager).expect("agents-notifier process should stop");

        assert_eq!(outcome, StopOutcome::Stopped { pid: 123 });
        assert_eq!(*manager.terminated.borrow(), vec![123]);
        assert!(!pid_file.exists());
    }

    #[test]
    fn refuses_to_stop_non_agents_notifier_process() {
        let dir = tempdir().expect("tempdir should be created");
        let pid_file = dir.path().join("agents-notifier.pid");
        fs::write(&pid_file, "123").expect("pid file should be written");
        let manager = FakeProcessManager::new(false);

        let err = stop_with_manager(&pid_file, &manager)
            .expect_err("non agents-notifier process should not be stopped");

        assert!(err.to_string().contains("not an agents-notifier process"));
        assert!(manager.terminated.borrow().is_empty());
        assert!(pid_file.exists());
    }
}
