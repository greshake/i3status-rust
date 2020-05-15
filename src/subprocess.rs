use std::io;
use std::process::{Command, Stdio};
use std::thread;

/// Spawns a new child process. This closes stdin and stdout, and returns to the caller after the
/// child has been started, while a background thread waits for the child to exit.
pub fn spawn_child_async(name: &str, args: &[&str]) -> io::Result<()> {
    let mut child = Command::new(name)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()?;
    thread::Builder::new()
        .name("subprocess".into())
        .spawn(move || child.wait())
        .unwrap();
    Ok(())
}
