use tokio::process::Command;

use super::super::packages::*;

pub struct Dnf;

impl Dnf {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Backend for Dnf {
    fn name(&self) -> &str {
        "dnf"
    }

    async fn get_updates_list(&self) -> Result<String> {
        let stdout = Command::new("sh")
            .env("LC_LANG", "C")
            .args(["-c", "dnf check-update -q --skip-broken"])
            .output()
            .await
            .error("Failed to run dnf check-update")?
            .stdout;
        String::from_utf8(stdout).error("dnf produced non-UTF8 output")
    }

    async fn get_update_count(&self, updates: &str) -> Result<usize> {
        Ok(updates.lines().filter(|line| line.len() > 1).count())
    }
}
