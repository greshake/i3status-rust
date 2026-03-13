use tokio::process::Command;

use super::*;

#[derive(Default)]
pub struct Zypper;

impl Zypper {
    pub fn new() -> Self {
        Default::default()
    }
}

#[async_trait]
impl Backend for Zypper {
    fn name(&self) -> Cow<'static, str> {
        "zypper".into()
    }

    async fn get_updates_list(&self) -> Result<Vec<String>> {
        let stdout = Command::new("zypper")
            .env("LC_ALL", "C")
            .args(["--quiet", "list-updates"])
            .output()
            .await
            .error("Failed to run `zypper list-updates`")?
            .stdout;

        let updates = String::from_utf8(stdout).error("zypper produced non-UTF8 output")?;
        let updates_list: Vec<String> = updates
            .lines()
            .filter(|line| line.starts_with("v"))
            .map(|line| line.to_string())
            .collect();

        Ok(updates_list)
    }
}
