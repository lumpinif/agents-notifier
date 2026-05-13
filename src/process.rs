use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, anyhow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopOutcome {
    MissingPidFile,
    RemovedStalePidFile { pid: u32 },
    Stopped { pid: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PidFileState {
    Missing,
    Stale { pid: u32 },
    AgentsRouter { pid: u32 },
    OtherProcess { pid: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessState {
    Missing,
    AgentsRouter,
    Other,
}

pub trait ProcessManager {
    fn process_state(&self, pid: u32) -> anyhow::Result<ProcessState>;
    fn terminate(&self, pid: u32) -> anyhow::Result<()>;
}

pub struct SystemProcessManager;

impl ProcessManager for SystemProcessManager {
    fn process_state(&self, pid: u32) -> anyhow::Result<ProcessState> {
        let output = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .with_context(|| format!("failed to inspect process `{pid}`"))?;

        if !output.status.success() {
            return Ok(ProcessState::Missing);
        }

        let command = String::from_utf8_lossy(&output.stdout);
        let name = Path::new(command.trim())
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        if name == "agents-router" {
            Ok(ProcessState::AgentsRouter)
        } else {
            Ok(ProcessState::Other)
        }
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
    let pid = match inspect_pid_file(pid_file_path, manager)? {
        PidFileState::Missing => return Ok(StopOutcome::MissingPidFile),
        PidFileState::Stale { pid } => {
            fs::remove_file(pid_file_path).with_context(|| {
                format!("failed to remove pid file `{}`", pid_file_path.display())
            })?;
            return Ok(StopOutcome::RemovedStalePidFile { pid });
        }
        PidFileState::AgentsRouter { pid } => pid,
        PidFileState::OtherProcess { pid } => {
            return Err(anyhow!(
                "pid file points to `{pid}`, but it is not an agents-router process"
            ));
        }
    };

    manager.terminate(pid)?;
    fs::remove_file(pid_file_path)
        .with_context(|| format!("failed to remove pid file `{}`", pid_file_path.display()))?;

    Ok(StopOutcome::Stopped { pid })
}

pub fn inspect_pid_file(
    pid_file_path: &Path,
    manager: &dyn ProcessManager,
) -> anyhow::Result<PidFileState> {
    if !pid_file_path.exists() {
        return Ok(PidFileState::Missing);
    }

    let raw_pid = fs::read_to_string(pid_file_path)
        .with_context(|| format!("failed to read pid file `{}`", pid_file_path.display()))?;
    let pid: u32 = raw_pid
        .trim()
        .parse()
        .with_context(|| format!("pid file `{}` is invalid", pid_file_path.display()))?;

    match manager.process_state(pid)? {
        ProcessState::Missing => Ok(PidFileState::Stale { pid }),
        ProcessState::AgentsRouter => Ok(PidFileState::AgentsRouter { pid }),
        ProcessState::Other => Ok(PidFileState::OtherProcess { pid }),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use tempfile::tempdir;

    use super::*;

    struct FakeProcessManager {
        process_state: ProcessState,
        terminated: RefCell<Vec<u32>>,
    }

    impl FakeProcessManager {
        fn new(process_state: ProcessState) -> Self {
            Self {
                process_state,
                terminated: RefCell::new(Vec::new()),
            }
        }
    }

    impl ProcessManager for FakeProcessManager {
        fn process_state(&self, _pid: u32) -> anyhow::Result<ProcessState> {
            Ok(self.process_state.clone())
        }

        fn terminate(&self, pid: u32) -> anyhow::Result<()> {
            self.terminated.borrow_mut().push(pid);
            Ok(())
        }
    }

    #[test]
    fn missing_pid_file_is_success() {
        let dir = tempdir().expect("tempdir should be created");
        let manager = FakeProcessManager::new(ProcessState::AgentsRouter);

        let outcome = stop_with_manager(&dir.path().join("missing.pid"), &manager)
            .expect("missing pid file should be successful");

        assert_eq!(outcome, StopOutcome::MissingPidFile);
        assert!(manager.terminated.borrow().is_empty());
    }

    #[test]
    fn inspects_agents_router_pid_file() {
        let dir = tempdir().expect("tempdir should be created");
        let pid_file = dir.path().join("agents-router.pid");
        fs::write(&pid_file, "123").expect("pid file should be written");
        let manager = FakeProcessManager::new(ProcessState::AgentsRouter);

        let state = inspect_pid_file(&pid_file, &manager).expect("pid file should be inspected");

        assert_eq!(state, PidFileState::AgentsRouter { pid: 123 });
    }

    #[test]
    fn inspects_stale_pid_file() {
        let dir = tempdir().expect("tempdir should be created");
        let pid_file = dir.path().join("agents-router.pid");
        fs::write(&pid_file, "123").expect("pid file should be written");
        let manager = FakeProcessManager::new(ProcessState::Missing);

        let state = inspect_pid_file(&pid_file, &manager).expect("pid file should be inspected");

        assert_eq!(state, PidFileState::Stale { pid: 123 });
    }

    #[test]
    fn inspects_other_process_pid_file() {
        let dir = tempdir().expect("tempdir should be created");
        let pid_file = dir.path().join("agents-router.pid");
        fs::write(&pid_file, "123").expect("pid file should be written");
        let manager = FakeProcessManager::new(ProcessState::Other);

        let state = inspect_pid_file(&pid_file, &manager).expect("pid file should be inspected");

        assert_eq!(state, PidFileState::OtherProcess { pid: 123 });
    }

    #[test]
    fn stops_agents_router_process_and_removes_pid_file() {
        let dir = tempdir().expect("tempdir should be created");
        let pid_file = dir.path().join("agents-router.pid");
        fs::write(&pid_file, "123").expect("pid file should be written");
        let manager = FakeProcessManager::new(ProcessState::AgentsRouter);

        let outcome =
            stop_with_manager(&pid_file, &manager).expect("agents-router process should stop");

        assert_eq!(outcome, StopOutcome::Stopped { pid: 123 });
        assert_eq!(*manager.terminated.borrow(), vec![123]);
        assert!(!pid_file.exists());
    }

    #[test]
    fn removes_stale_pid_file() {
        let dir = tempdir().expect("tempdir should be created");
        let pid_file = dir.path().join("agents-router.pid");
        fs::write(&pid_file, "123").expect("pid file should be written");
        let manager = FakeProcessManager::new(ProcessState::Missing);

        let outcome = stop_with_manager(&pid_file, &manager)
            .expect("stale agents-router pid file should be removed");

        assert_eq!(outcome, StopOutcome::RemovedStalePidFile { pid: 123 });
        assert!(!pid_file.exists());
        assert!(manager.terminated.borrow().is_empty());
    }

    #[test]
    fn refuses_to_stop_non_agents_router_process() {
        let dir = tempdir().expect("tempdir should be created");
        let pid_file = dir.path().join("agents-router.pid");
        fs::write(&pid_file, "123").expect("pid file should be written");
        let manager = FakeProcessManager::new(ProcessState::Other);

        let err = stop_with_manager(&pid_file, &manager)
            .expect_err("non agents-router process should not be stopped");

        assert!(err.to_string().contains("not an agents-router process"));
        assert!(manager.terminated.borrow().is_empty());
        assert!(pid_file.exists());
    }
}
