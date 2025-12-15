use tokio::process::Command;

use super::*;

#[derive(Default)]
pub struct Apk;

impl Apk {
    pub fn new() -> Self {
        Default::default()
    }
}

#[async_trait]
impl Backend for Apk {
    fn name(&self) -> Cow<'static, str> {
        "apk".into()
    }

    async fn get_updates_list(&self) -> Result<Vec<String>> {
        let stdout = Command::new("apk")
            .env("LC_LANG", "C")
            .args(["--no-cache", "-q", "list", "--upgradable"])
            .output()
            .await
            .error("Problem running apk command")?
            .stdout;

        let updates = String::from_utf8(stdout).expect("apk produced non-UTF8 output");
        let updates_list: Vec<String> = updates
            .lines()
            .filter(|line| line.len() > 1)
            .map(|line| line.to_string())
            .collect();

        Ok(updates_list)
    }
}
