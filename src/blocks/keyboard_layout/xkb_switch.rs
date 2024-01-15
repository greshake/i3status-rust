use super::*;
use tokio::process::Command;

pub(super) struct XkbSwitch(Seconds);

impl XkbSwitch {
    pub(super) fn new(update_interval: Seconds) -> Self {
        Self(update_interval)
    }
}

#[async_trait]
impl Backend for XkbSwitch {
    async fn get_info(&mut self) -> Result<Info> {
        // This command output is in the format of "layout(variant)" or "layout"
        let output = Command::new("xkb-switch")
            .arg("-p")
            .output()
            .await
            .error("Failed to execute 'xkb-switch -p'")?;

        let output =
            String::from_utf8(output.stdout).error("xkb-switch produces a non-UTF8 output")?;

        let mut components = output.trim_end().split('(');

        let layout = components
            .next()
            .error("Could not find layout entry in xkb-switch")?
            .to_string();

        let variant = components
            .last()
            // Remove the trailing parenthesis ")"
            .map(|variant_str| variant_str.split_at(variant_str.len() - 1).0.to_string());

        Ok(Info { layout, variant })
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        sleep(self.0 .0).await;
        Ok(())
    }
}
