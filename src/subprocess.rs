use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

/// Spawn a new detached process
pub fn spawn_process(cmd: &str, args: &[&str]) -> io::Result<()> {
    let mut proc = Command::new(cmd);
    proc.args(args);
    proc.stdin(Stdio::null());
    proc.stdout(Stdio::null());
    // Safety: libc::daemon() is async-signal-safe
    unsafe {
        proc.pre_exec(|| match libc::daemon(0, 0) {
            -1 => Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to detach new process",
            )),
            _ => Ok(()),
        });
    }
    proc.spawn()?.wait()?;
    Ok(())
}

/// Spawn a new detached shell
pub fn spawn_shell(cmd: &str) -> io::Result<()> {
    spawn_process("sh", &["-c", cmd])
}

pub async fn spawn_shell_sync(cmd: &str) -> io::Result<()> {
    tokio::process::Command::new("sh")
        .args(["-c", cmd])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()?
        .wait()
        .await?;
    Ok(())
}
