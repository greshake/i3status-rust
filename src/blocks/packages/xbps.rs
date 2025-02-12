use tokio::process::Command;

use super::*;

#[derive(Default)]
pub struct Xbps;

impl Xbps {
    pub fn new() -> Self {
        Default::default()
    }
}

#[async_trait]
impl Backend for Xbps {
    fn name(&self) -> Cow<'static, str> {
        "xbps".into()
    }

    async fn get_updates_list(&self) -> Result<Vec<String>> {
        let stdout = Command::new("xbps-install")
            .env("LC_LANG", "C")
            .args(["-M", "-u", "-n"])
            .output()
            .await
            .error("Problem running xbps-install command")?
            .stdout;

        let updates = String::from_utf8(stdout).expect("xbps-install produced non-UTF8 output");
        let updates_list: Vec<String> = updates
            .lines()
            .filter(|line| line.len() > 1)
            .map(|line| line.to_string())
            .collect();

        Ok(updates_list)
    }
}
