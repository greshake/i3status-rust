use tokio::process::Command;

use super::*;

#[derive(Default)]
pub struct Flatpak;

impl Flatpak {
    pub fn new() -> Self {
        Default::default()
    }
}

#[async_trait]
impl Backend for Flatpak {
    fn name(&self) -> Cow<'static, str> {
        "flatpak".into()
    }

    async fn get_updates_list(&self) -> Result<Vec<String>> {
        Command::new("flatpak")
            .env("LC_ALL", "C")
            .args(["update", "--appstream", "-y"])
            .output()
            .await
            .error("Failed to run `flatpak update`")?;

        let stdout = Command::new("flatpak")
            .env("LC_ALL", "C")
            .args(["remote-ls", "--updates", "--columns=ref"])
            .output()
            .await
            .error("Failed to run `flatpak remote-ls`")?
            .stdout;

        let updates = String::from_utf8(stdout)
            .error("flatpak produced non-UTF8 output")?
            .lines()
            .filter_map(|line| (line.len() > 1).then_some(line.to_string()))
            .collect();

        Ok(updates)
    }
}
