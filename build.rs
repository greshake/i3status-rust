use std::process::Command;

fn main() {
    let output = Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output()
        .unwrap();
    let hash = String::from_utf8(output.stdout).unwrap();
    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", hash);

    let output = Command::new("git")
        .args(&["log", "--pretty=format:'%ad'", "-n1", "--date=short"])
        .output()
        .unwrap();
    let date = String::from_utf8(output.stdout)
        .unwrap()
        .trim_matches('\'')
        .to_string();
    println!("cargo:rustc-env=GIT_COMMIT_DATE={}", date);
}
