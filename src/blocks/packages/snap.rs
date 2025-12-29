use tokio::process::Command;

use super::*;

#[derive(Default)]
pub struct Snap;

impl Snap {
    pub fn new() -> Self {
        Default::default()
    }
}

#[async_trait]
impl Backend for Snap {
    fn name(&self) -> Cow<'static, str> {
        "snap".into()
    }

    async fn get_updates_list(&self) -> Result<Vec<String>> {
        let stdout = Command::new("sh")
            .env("LC_LANG", "C")
            .args(["-c", "snap refresh --list"])
            .output()
            .await
            .error("Failed to run `snap refresh`")?
            .stdout;

        let updates = String::from_utf8(stdout)
            .error("snap produced non-UTF8 output")?
            .lines()
            .filter_map(|line| (line.len() > 1).then_some(line.to_string()))
            .collect();

        Ok(updates)
    }
}
