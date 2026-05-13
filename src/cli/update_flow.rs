#[cfg(windows)]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fmt;
#[cfg(windows)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use console::style;

use agents_notifier::config::CliLanguage;
use agents_notifier::i18n::I18n;
use agents_notifier::update::{self, AppVersion, SKIP_UPDATE_CHECK_ENV, UpdateInstallMethod};

use super::setup_flow::prompt_confirm;

const NPM_PACKAGE_NAME: &str = "agents-notifier";
const OFFICIAL_REPO: &str = "lumpinif/agents-notifier";

pub(super) enum SetupUpdateOutcome {
    Continue,
    Exit(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(windows),
    allow(
        dead_code,
        reason = "non-host platform variants keep update command construction testable on every OS"
    )
)]
enum UpdateRuntimePlatform {
    Unix,
    Windows,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct UpdateCommandContext {
    install_dir: Option<PathBuf>,
    current_exe: Option<PathBuf>,
    home: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UpdateCommand {
    program: OsString,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
}

impl UpdateCommand {
    fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
        }
    }

    fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    fn skip_update_check(mut self) -> Self {
        self.env
            .push((OsString::from(SKIP_UPDATE_CHECK_ENV), OsString::from("1")));
        self
    }

    fn env_var(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

impl fmt::Display for UpdateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.program.to_string_lossy())?;
        for arg in &self.args {
            write!(formatter, " {}", arg.to_string_lossy())?;
        }
        Ok(())
    }
}

pub(super) async fn maybe_update_before_setup(
    config_path: &Path,
    i18n: I18n,
) -> anyhow::Result<SetupUpdateOutcome> {
    if update::should_skip_update_check() {
        return Ok(SetupUpdateOutcome::Continue);
    }

    let current_exe = std::env::current_exe().context("failed to locate current executable")?;
    let current_version = AppVersion::parse(env!("CARGO_PKG_VERSION"))
        .context("failed to parse current agents-notifier version")?;
    let platform = current_update_runtime_platform();
    let context = current_update_command_context(&current_exe);
    let install_method_marker = match update::read_install_method_marker(&current_exe) {
        Ok(marker) => marker,
        Err(error) => {
            print_update_check_failure(i18n, current_version, &error);
            return Ok(SetupUpdateOutcome::Continue);
        }
    };
    let known_script_binary_path = known_script_binary_path(platform, &context);
    let install_method = match update::resolve_update_install_method(
        &current_exe,
        std::env::var(update::INSTALL_METHOD_ENV).ok().as_deref(),
        install_method_marker.as_deref(),
        known_script_binary_path.as_deref(),
    ) {
        Some(method) => method,
        None => return Ok(SetupUpdateOutcome::Continue),
    };

    let latest_version =
        match update::fetch_latest_version(install_method.latest_version_source()).await {
            Ok(version) => version,
            Err(error) => {
                print_update_check_failure(i18n, current_version, &error);
                return Ok(SetupUpdateOutcome::Continue);
            }
        };

    if !update::update_is_available(current_version, latest_version) {
        return Ok(SetupUpdateOutcome::Continue);
    }

    print_update_available(i18n, current_version, latest_version);
    if !prompt_confirm(update_before_setup_prompt(i18n), true)? {
        return Ok(SetupUpdateOutcome::Continue);
    }

    let commands = build_setup_update_commands(
        install_method,
        latest_version,
        config_path,
        platform,
        &context,
    )?;

    println!("{}", update_starting_message(i18n));
    let exit_code = execute_setup_update(commands)?;
    Ok(SetupUpdateOutcome::Exit(exit_code))
}

fn print_update_check_failure(i18n: I18n, current_version: AppVersion, error: &anyhow::Error) {
    match i18n.language() {
        CliLanguage::English => {
            println!(
                "{} {}",
                style("Could not check for updates:").yellow(),
                error
            );
            println!("Continuing with agents-notifier {current_version}.");
        }
        CliLanguage::SimplifiedChinese => {
            println!("{} {}", style("检查更新失败：").yellow(), error);
            println!("继续使用 agents-notifier {current_version}。");
        }
    }
    println!();
}

fn print_update_available(i18n: I18n, current_version: AppVersion, latest_version: AppVersion) {
    match i18n.language() {
        CliLanguage::English => {
            println!("{}", style("A newer version is available.").yellow().bold());
            println!();
            println!("Current version: {current_version}");
            println!("Latest version:  {latest_version}");
        }
        CliLanguage::SimplifiedChinese => {
            println!("{}", style("发现新版本。").yellow().bold());
            println!();
            println!("当前版本：{current_version}");
            println!("最新版本：{latest_version}");
        }
    }
    println!();
}

fn update_before_setup_prompt(i18n: I18n) -> &'static str {
    match i18n.language() {
        CliLanguage::English => "Update before setup?",
        CliLanguage::SimplifiedChinese => "要先更新再继续 setup 吗？",
    }
}

fn update_starting_message(i18n: I18n) -> &'static str {
    match i18n.language() {
        CliLanguage::English => "Updating Agents Notifier...",
        CliLanguage::SimplifiedChinese => "正在更新 Agents Notifier...",
    }
}

fn current_update_runtime_platform() -> UpdateRuntimePlatform {
    #[cfg(windows)]
    return UpdateRuntimePlatform::Windows;

    #[cfg(not(windows))]
    return UpdateRuntimePlatform::Unix;
}

fn current_update_command_context(current_exe: &Path) -> UpdateCommandContext {
    UpdateCommandContext {
        install_dir: non_empty_env_path("AGENTS_NOTIFIER_INSTALL_DIR"),
        current_exe: Some(current_exe.to_path_buf()),
        home: non_empty_env_path("HOME"),
        local_app_data: non_empty_env_path("LOCALAPPDATA"),
    }
}

fn non_empty_env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn build_setup_update_commands(
    install_method: UpdateInstallMethod,
    latest_version: AppVersion,
    config_path: &Path,
    platform: UpdateRuntimePlatform,
    context: &UpdateCommandContext,
) -> anyhow::Result<Vec<UpdateCommand>> {
    match install_method {
        UpdateInstallMethod::Npx => {
            let mut command = UpdateCommand::new("npx")
                .arg("--yes")
                .arg("--prefer-online")
                .arg(format!("{NPM_PACKAGE_NAME}@{latest_version}"))
                .arg("setup")
                .arg("--config")
                .arg(config_path.as_os_str())
                .skip_update_check();
            if let Some(install_dir) = current_binary_install_dir(context) {
                command = command.env_var("AGENTS_NOTIFIER_INSTALL_DIR", install_dir.as_os_str());
            }
            Ok(vec![command])
        }
        UpdateInstallMethod::Npm => {
            let mut commands = Vec::new();
            if platform == UpdateRuntimePlatform::Windows {
                commands.push(UpdateCommand::new("agents-notifier").arg("stop"));
            }
            commands.push(
                UpdateCommand::new("npm")
                    .arg("install")
                    .arg("-g")
                    .arg(format!("{NPM_PACKAGE_NAME}@{latest_version}")),
            );
            commands.push(
                UpdateCommand::new("agents-notifier")
                    .arg("setup")
                    .arg("--config")
                    .arg(config_path.as_os_str())
                    .skip_update_check(),
            );
            Ok(commands)
        }
        UpdateInstallMethod::Script => {
            let install_dir = stable_script_install_dir(platform, context)?;
            let binary_path = install_dir.join(binary_name(platform));
            let release_tag = release_tag(latest_version);
            let install_command = match platform {
                UpdateRuntimePlatform::Unix => UpdateCommand::new("sh")
                    .arg("-c")
                    .arg(install_sh_command(&release_tag)),
                UpdateRuntimePlatform::Windows => UpdateCommand::new("powershell.exe")
                    .arg("-NoProfile")
                    .arg("-ExecutionPolicy")
                    .arg("Bypass")
                    .arg("-Command")
                    .arg(install_ps1_command(&release_tag)),
            }
            .env_var("AGENTS_NOTIFIER_REPO", OFFICIAL_REPO)
            .env_var("AGENTS_NOTIFIER_VERSION", release_tag)
            .env_var("AGENTS_NOTIFIER_INSTALL_DIR", install_dir.as_os_str());

            Ok(vec![
                install_command,
                UpdateCommand::new(binary_path)
                    .arg("setup")
                    .arg("--config")
                    .arg(config_path.as_os_str())
                    .skip_update_check(),
            ])
        }
    }
}

fn stable_script_install_dir(
    platform: UpdateRuntimePlatform,
    context: &UpdateCommandContext,
) -> anyhow::Result<PathBuf> {
    if let Some(install_dir) = &context.install_dir {
        return Ok(install_dir.clone());
    }

    if let Some(current_exe) = &context.current_exe
        && let Some(parent) = current_exe.parent()
    {
        return Ok(parent.to_path_buf());
    }

    match platform {
        UpdateRuntimePlatform::Unix => {
            let home = context.home.as_ref().context(
                "HOME is not set; set AGENTS_NOTIFIER_INSTALL_DIR to choose an install directory",
            )?;
            Ok(home.join(".local").join("bin"))
        }
        UpdateRuntimePlatform::Windows => {
            let local_app_data = context.local_app_data.as_ref().context(
                "LOCALAPPDATA is not set; set AGENTS_NOTIFIER_INSTALL_DIR to choose an install directory",
            )?;
            Ok(local_app_data.join("Programs").join("agents-notifier"))
        }
    }
}

fn current_binary_install_dir(context: &UpdateCommandContext) -> Option<PathBuf> {
    context.install_dir.clone().or_else(|| {
        context
            .current_exe
            .as_ref()?
            .parent()
            .map(Path::to_path_buf)
    })
}

fn known_script_binary_path(
    platform: UpdateRuntimePlatform,
    context: &UpdateCommandContext,
) -> Option<PathBuf> {
    if let Some(install_dir) = &context.install_dir {
        return Some(install_dir.join(binary_name(platform)));
    }

    match platform {
        UpdateRuntimePlatform::Unix => context
            .home
            .as_ref()
            .map(|home| home.join(".local").join("bin").join(binary_name(platform))),
        UpdateRuntimePlatform::Windows => context.local_app_data.as_ref().map(|local_app_data| {
            local_app_data
                .join("Programs")
                .join("agents-notifier")
                .join(binary_name(platform))
        }),
    }
}

fn binary_name(platform: UpdateRuntimePlatform) -> &'static str {
    match platform {
        UpdateRuntimePlatform::Unix => "agents-notifier",
        UpdateRuntimePlatform::Windows => "agents-notifier.exe",
    }
}

fn release_tag(version: AppVersion) -> String {
    format!("v{version}")
}

fn install_sh_command(release_tag: &str) -> String {
    format!(
        "curl -fsSL https://raw.githubusercontent.com/{OFFICIAL_REPO}/{release_tag}/install.sh | sh"
    )
}

fn install_ps1_command(release_tag: &str) -> String {
    format!("irm https://raw.githubusercontent.com/{OFFICIAL_REPO}/{release_tag}/install.ps1 | iex")
}

#[cfg(not(windows))]
fn execute_setup_update(commands: Vec<UpdateCommand>) -> anyhow::Result<i32> {
    run_update_commands(commands)
}

#[cfg(windows)]
fn execute_setup_update(commands: Vec<UpdateCommand>) -> anyhow::Result<i32> {
    start_windows_update_helper(commands)?;
    Ok(0)
}

#[cfg(not(windows))]
fn run_update_commands(commands: Vec<UpdateCommand>) -> anyhow::Result<i32> {
    let last_index = commands.len().saturating_sub(1);
    for (index, command) in commands.iter().enumerate() {
        let status = Command::new(&command.program)
            .args(&command.args)
            .envs(command.env.iter().map(|(key, value)| (key, value)))
            .status()
            .with_context(|| format!("failed to run update command `{command}`"))?;

        if status.success() {
            continue;
        }

        let code = status.code().unwrap_or(1);
        if index == last_index {
            return Ok(code);
        }

        anyhow::bail!("update command failed with exit code {code}: `{command}`");
    }

    Ok(0)
}

#[cfg(windows)]
fn start_windows_update_helper(commands: Vec<UpdateCommand>) -> anyhow::Result<()> {
    let script_path = windows_update_helper_path()?;
    let script = build_windows_update_helper_script(&commands, std::process::id(), &script_path);
    fs::write(&script_path, script)
        .with_context(|| format!("failed to write update helper `{}`", script_path.display()))?;

    Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script_path)
        .spawn()
        .with_context(|| format!("failed to start update helper `{}`", script_path.display()))?;

    println!("Update will continue in a PowerShell helper after this process exits.");
    Ok(())
}

#[cfg(windows)]
fn windows_update_helper_path() -> anyhow::Result<PathBuf> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_millis();
    Ok(std::env::temp_dir().join(format!(
        "agents-notifier-update-{}-{now}.ps1",
        std::process::id()
    )))
}

#[cfg(windows)]
fn build_windows_update_helper_script(
    commands: &[UpdateCommand],
    parent_pid: u32,
    script_path: &Path,
) -> String {
    let mut script = String::new();
    script.push_str("$ErrorActionPreference = 'Stop'\n");
    script.push_str(&format!(
        "Wait-Process -Id {parent_pid} -ErrorAction SilentlyContinue\n"
    ));

    for command in commands {
        for (key, value) in &command.env {
            script.push_str(&format!(
                "$env:{} = {}\n",
                key.to_string_lossy(),
                powershell_quote(value)
            ));
        }

        script.push_str("& ");
        script.push_str(&powershell_quote(&command.program));
        for arg in &command.args {
            script.push(' ');
            script.push_str(&powershell_quote(arg));
        }
        script.push('\n');
        script.push_str("if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n");
    }

    script.push_str(&format!(
        "Remove-Item -LiteralPath {} -Force -ErrorAction SilentlyContinue\n",
        powershell_quote(script_path.as_os_str())
    ));
    script
}

#[cfg(windows)]
fn powershell_quote(value: &OsStr) -> String {
    format!("'{}'", value.to_string_lossy().replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use agents_notifier::update::AppVersion;

    use super::*;

    #[test]
    fn npx_update_command_runs_latest_setup_with_config() {
        let context = UpdateCommandContext {
            current_exe: Some(PathBuf::from("/Users/tester/.local/bin/agents-notifier")),
            ..UpdateCommandContext::default()
        };

        let commands = build_setup_update_commands(
            UpdateInstallMethod::Npx,
            AppVersion::parse("0.8.0").unwrap(),
            Path::new("/tmp/config.toml"),
            UpdateRuntimePlatform::Unix,
            &context,
        )
        .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].program, OsString::from("npx"));
        assert_eq!(
            commands[0].args,
            vec![
                OsString::from("--yes"),
                OsString::from("--prefer-online"),
                OsString::from("agents-notifier@0.8.0"),
                OsString::from("setup"),
                OsString::from("--config"),
                OsString::from("/tmp/config.toml"),
            ]
        );
        assert!(commands[0].env.iter().any(|(key, value)| {
            key == &OsString::from(SKIP_UPDATE_CHECK_ENV) && value == &OsString::from("1")
        }));
        assert!(commands[0].env.iter().any(|(key, value)| {
            key == &OsString::from("AGENTS_NOTIFIER_INSTALL_DIR")
                && value == &OsString::from("/Users/tester/.local/bin")
        }));
    }

    #[test]
    fn npm_update_commands_preserve_config() {
        let commands = build_setup_update_commands(
            UpdateInstallMethod::Npm,
            AppVersion::parse("0.8.0").unwrap(),
            Path::new("/tmp/config.toml"),
            UpdateRuntimePlatform::Unix,
            &UpdateCommandContext::default(),
        )
        .unwrap();

        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].program, OsString::from("npm"));
        assert_eq!(commands[0].args[0], OsString::from("install"));
        assert_eq!(commands[0].args[1], OsString::from("-g"));
        assert_eq!(commands[0].args[2], OsString::from("agents-notifier@0.8.0"));
        assert_eq!(commands[1].program, OsString::from("agents-notifier"));
        assert_eq!(commands[1].args[0], OsString::from("setup"));
        assert_eq!(commands[1].args[1], OsString::from("--config"));
        assert_eq!(commands[1].args[2], OsString::from("/tmp/config.toml"));
    }

    #[test]
    fn windows_npm_update_stops_service_before_installing() {
        let commands = build_setup_update_commands(
            UpdateInstallMethod::Npm,
            AppVersion::parse("0.8.0").unwrap(),
            Path::new(r"C:\Users\tester\config.toml"),
            UpdateRuntimePlatform::Windows,
            &UpdateCommandContext::default(),
        )
        .unwrap();

        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].program, OsString::from("agents-notifier"));
        assert_eq!(commands[0].args, vec![OsString::from("stop")]);
        assert_eq!(commands[1].program, OsString::from("npm"));
        assert_eq!(commands[2].program, OsString::from("agents-notifier"));
        assert_eq!(commands[2].args[0], OsString::from("setup"));
    }

    #[test]
    fn unix_script_update_uses_stable_binary_path_and_preserves_config() {
        let context = UpdateCommandContext {
            home: Some(PathBuf::from("/Users/tester")),
            ..UpdateCommandContext::default()
        };

        let commands = build_setup_update_commands(
            UpdateInstallMethod::Script,
            AppVersion::parse("0.8.0").unwrap(),
            Path::new("/tmp/config.toml"),
            UpdateRuntimePlatform::Unix,
            &context,
        )
        .unwrap();

        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].program, OsString::from("sh"));
        assert_eq!(commands[0].args[0], OsString::from("-c"));
        assert_eq!(
            commands[0].args[1],
            OsString::from(
                "curl -fsSL https://raw.githubusercontent.com/lumpinif/agents-notifier/v0.8.0/install.sh | sh"
            )
        );
        assert!(commands[0].env.iter().any(|(key, value)| {
            key == &OsString::from("AGENTS_NOTIFIER_VERSION") && value == &OsString::from("v0.8.0")
        }));
        assert!(commands[0].env.iter().any(|(key, value)| {
            key == &OsString::from("AGENTS_NOTIFIER_INSTALL_DIR")
                && value == &OsString::from("/Users/tester/.local/bin")
        }));
        assert_eq!(
            commands[1].program,
            OsString::from("/Users/tester/.local/bin/agents-notifier")
        );
        assert_eq!(commands[1].args[0], OsString::from("setup"));
        assert_eq!(commands[1].args[1], OsString::from("--config"));
        assert_eq!(commands[1].args[2], OsString::from("/tmp/config.toml"));
    }

    #[test]
    fn windows_script_update_uses_stable_binary_path_and_preserves_config() {
        let context = UpdateCommandContext {
            local_app_data: Some(PathBuf::from(r"C:\Users\tester\AppData\Local")),
            ..UpdateCommandContext::default()
        };

        let commands = build_setup_update_commands(
            UpdateInstallMethod::Script,
            AppVersion::parse("0.8.0").unwrap(),
            Path::new(r"C:\Users\tester\config.toml"),
            UpdateRuntimePlatform::Windows,
            &context,
        )
        .unwrap();

        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].program, OsString::from("powershell.exe"));
        assert!(
            commands[0]
                .args
                .iter()
                .any(|arg| arg
                    == "irm https://raw.githubusercontent.com/lumpinif/agents-notifier/v0.8.0/install.ps1 | iex")
        );
        assert_eq!(
            commands[1].program,
            OsString::from(
                r"C:\Users\tester\AppData\Local/Programs/agents-notifier/agents-notifier.exe"
            )
        );
        assert_eq!(commands[1].args[0], OsString::from("setup"));
        assert_eq!(commands[1].args[1], OsString::from("--config"));
        assert_eq!(
            commands[1].args[2],
            OsString::from(r"C:\Users\tester\config.toml")
        );
    }
}
