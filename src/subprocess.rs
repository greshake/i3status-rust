use crate::errors::{Error, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Output, Stdio, id};
use std::sync::OnceLock;
use std::{env, io};

pub const ENV_VAR_PID: &str = "I3STATUS_RS_PID";
pub const ENV_VAR_CONFIG: &str = "I3STATUS_RS_CONFIG";

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct SubprocessConfig {
    #[serde(default)]
    pub add_pid: Option<bool>,
    #[serde(default)]
    pub add_config_file_path: Option<bool>,
    #[serde(default)]
    pub environment: HashMap<String, String>,
}

static SUBPROCESS_ENV: OnceLock<HashMap<OsString, OsString>> = OnceLock::new();

pub fn subprocess_init(config: &SubprocessConfig, config_file: impl AsRef<OsStr>) -> Result<()> {
    let mut env: HashMap<OsString, OsString> = HashMap::with_capacity(2 + config.environment.len());

    env.extend(config.environment.iter().map(|(k, v)| (k.into(), v.into())));

    fn insert_env(
        env: &mut HashMap<OsString, OsString>,
        key: &str,
        value: OsString,
        option_name: &str,
    ) -> Result<()> {
        if env.insert(key.into(), value).is_some() {
            Err(Error::new(format!(
                "Cannot specify {key} in subprocess environment when subprocess.{option_name} is set"
            )))
        } else {
            Ok(())
        }
    }

    if config.add_pid.unwrap_or(true) {
        insert_env(&mut env, ENV_VAR_PID, id().to_string().into(), "add_pid")?;
    }

    if config.add_config_file_path.unwrap_or(true) {
        insert_env(
            &mut env,
            ENV_VAR_CONFIG,
            config_file.as_ref().into(),
            "add_config_file_path",
        )?;
    }

    SUBPROCESS_ENV
        .set(env)
        .map_err(|_| Error::new("Subprocess environment already initialized"))
}

fn get_subprocess_env() -> io::Result<&'static HashMap<OsString, OsString>> {
    SUBPROCESS_ENV
        .get()
        .ok_or_else(|| io::Error::other("Subprocess environment not initialized"))
}

pub trait CommandExt {
    fn with_environment(&mut self) -> io::Result<&mut Self>;
}

impl CommandExt for Command {
    fn with_environment(&mut self) -> io::Result<&mut Self> {
        self.envs(get_subprocess_env()?);
        Ok(self)
    }
}

impl CommandExt for tokio::process::Command {
    fn with_environment(&mut self) -> io::Result<&mut Self> {
        self.envs(get_subprocess_env()?);
        Ok(self)
    }
}

/// Spawn a new detached process
pub fn spawn_process(cmd: &str, args: &[&str]) -> io::Result<()> {
    let mut proc = Command::new(cmd);
    proc.args(args);
    proc.stdin(Stdio::null());
    proc.stdout(Stdio::null());
    proc.with_environment()?;
    // Safety: libc::daemon() is async-signal-safe
    unsafe {
        proc.pre_exec(|| match libc::daemon(0, 0) {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        });
    }
    proc.spawn()?.wait()?;
    Ok(())
}

/// Spawn a new detached shell
pub fn spawn_shell(cmd: &str) -> io::Result<()> {
    spawn_process(&get_shell(), &["-c", cmd])
}

pub async fn spawn_shell_sync(cmd: &str) -> io::Result<()> {
    tokio::process::Command::new(get_shell())
        .args(["-c", cmd])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .with_environment()?
        .spawn()?
        .wait()
        .await?;
    Ok(())
}

pub fn get_shell() -> String {
    env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
}

pub async fn get_output(shell_command: &str) -> io::Result<Output> {
    tokio::process::Command::new(get_shell())
        .args(["-c", shell_command])
        .stdin(Stdio::null())
        .with_environment()?
        .output()
        .await
}
