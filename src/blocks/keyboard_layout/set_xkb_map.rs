use super::*;
use tokio::process::Command;

pub(super) struct SetXkbMap(Seconds);

impl SetXkbMap {
    pub(super) fn new(update_interval: Seconds) -> Self {
        Self(update_interval)
    }
}

#[async_trait]
impl Backend for SetXkbMap {
    async fn get_info(&mut self) -> Result<Info> {
        let output = Command::new("setxkbmap")
            .arg("-query")
            .output()
            .await
            .error("Failed to execute setxkbmap")?;
        let output =
            String::from_utf8(output.stdout).error("setxkbmap produced a non-UTF8 output")?;
        let layout = output
            .lines()
            // Find the "layout:    xxxx" entry.
            .find(|line| line.starts_with("layout"))
            .error("Could not find the layout entry from setxkbmap")?
            .split_ascii_whitespace()
            .last()
            .error("Could not read the layout entry from setxkbmap.")?
            .into();
        let variant_line = output
            .lines()
            // Find the "variant:   xxxx" line if it exists.
            .find(|line| line.starts_with("variant"));
        let variant = match variant_line {
            Some(s) => Some(
                s.split_ascii_whitespace()
                    .last()
                    .error("Could not read the variant entry from setxkbmap.")?
                    .to_string(),
            ),
            None => None,
        };

        Ok(Info { layout, variant })
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        sleep(self.0 .0).await;
        Ok(())
    }
}
