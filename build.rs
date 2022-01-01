use std::process::Command;

fn main() {
    let output = Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output();
    let hash = match output {
        Ok(o) => String::from_utf8(o.stdout).unwrap(),
        Err(_) => String::from(""),
    };
    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", hash);

    let output = Command::new("git")
        .args(&["log", "--pretty=format:'%ad'", "-n1", "--date=short"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output();
    let date = match output {
        Ok(o) => String::from_utf8(o.stdout)
            .unwrap()
            .trim_matches('\'')
            .to_string(),
        Err(_) => String::from(""),
    };
    println!("cargo:rustc-env=GIT_COMMIT_DATE={}", date);
}
