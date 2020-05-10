#[cfg(test)]
mod run_binary {
    use std::process::Command;

    #[test]
    fn debug_build() {
        let output = Command::new("./target/debug/i3status-rs")
            .args(&["--one-shot", "./tests/testconfig1.toml"])
            .status()
            .expect("failed to execute process");
        assert_eq!(output.success(), true);
    }

    #[test]
    fn release_build() {
        let output = Command::new("./target/release/i3status-rs")
            .args(&["--one-shot", "./tests/testconfig1.toml"])
            .status()
            .expect("failed to execute process");
        assert_eq!(output.success(), true);
    }
}
