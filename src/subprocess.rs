use std::io;
use std::process::{Command, Stdio};
use std::thread;

/// Splits a string into command name and arguments.
pub fn parse_command(command: &str) -> (&str, Vec<&str>) {
    let components: Vec<&str> = command.split_whitespace().collect();
    let (names, args) = components.split_at(1);
    let name = names.get(0).unwrap();
    (name, args.to_vec())
}

/// Spawns a new child process. This closes stdin and stdout, and returns to the caller after the
/// child has been started, while a background thread waits for the child to exit.
pub fn spawn_child_async(name: &str, args: &[&str]) -> io::Result<()> {
    let mut child = Command::new(name)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()?;
    thread::spawn(move || child.wait());
    Ok(())
}
