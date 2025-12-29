use tokio::process::Command;

use super::*;

#[derive(Default)]
pub struct Brew;

impl Brew {
    pub fn new() -> Self {
        Default::default()
    }
}

#[async_trait]
impl Backend for Brew {
    fn name(&self) -> Cow<'static, str> {
        "brew".into()
    }

    async fn get_updates_list(&self) -> Result<Vec<String>> {
        let stdout = Command::new("sh")
            .env("LC_LANG", "C")
            .args(["-c", "brew outdated"])
            .output()
            .await
            .error("Failed to run `brew outdated`")?
            .stdout;

        let updates = String::from_utf8(stdout)
            .error("brew produced non-UTF8 output")?
            .lines()
            .filter_map(|line| (line.len() > 1).then_some(line.to_string()))
            .collect();

        Ok(updates)
    }
}
