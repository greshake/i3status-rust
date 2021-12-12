use super::{I3BarWidget, Spacing, State};
use crate::config::SharedConfig;
use crate::errors::*;
use crate::protocol::i3bar_block::I3BarBlock;

#[derive(Clone, Debug)]
pub struct TextWidget {
    pub instance: usize,
    content: Option<String>,
    content_short: Option<String>,
    icon: Option<String>,
    state: State,
    spacing: Spacing,
    spacing_short: Spacing,
    shared_config: SharedConfig,
    inner: I3BarBlock,
}

impl TextWidget {
    pub fn new(id: usize, instance: usize, shared_config: SharedConfig) -> Self {
        let (key_bg, key_fg) = State::Idle.theme_keys(&shared_config.theme); // Initial colors
        let inner = I3BarBlock {
            name: Some(id.to_string()),
            instance: Some(instance.to_string()),
            color: key_fg,
            background: key_bg,
            ..I3BarBlock::default()
        };

        TextWidget {
            instance,
            content: None,
            content_short: None,
            icon: None,
            state: State::Idle,
            spacing: Spacing::Normal,
            spacing_short: Spacing::Normal,
            shared_config,
            inner,
        }
    }

    pub fn with_icon(mut self, name: &str) -> Result<Self> {
        self.icon = Some(self.shared_config.get_icon(name)?);
        self.update();
        Ok(self)
    }

    pub fn with_text(mut self, content: &str) -> Self {
        self.content = Some(String::from(content));
        self.update();
        self
    }

    pub fn with_state(mut self, state: State) -> Self {
        self.state = state;
        self.update();
        self
    }

    pub fn with_spacing(mut self, spacing: Spacing) -> Self {
        self.spacing = spacing;
        self.spacing_short = spacing;
        self.update();
        self
    }

    pub fn set_icon(&mut self, name: &str) -> Result<()> {
        self.icon = Some(self.shared_config.get_icon(name)?);
        self.update();
        Ok(())
    }

    pub fn unset_icon(&mut self) {
        self.icon = None;
        self.update();
    }

    pub fn set_text(&mut self, content: String) {
        self.set_texts((content, None));
    }

    pub fn set_texts(&mut self, contents: (String, Option<String>)) {
        self.spacing = Spacing::from_content(&contents.0);
        self.spacing_short = if let Some(ref short) = contents.1 {
            Spacing::from_content(short)
        } else {
            self.spacing
        };
        self.content = Some(contents.0);
        self.content_short = contents.1;
        self.update();
    }

    pub fn set_state(&mut self, state: State) {
        self.state = state;
        self.update();
    }

    pub fn set_spacing(&mut self, spacing: Spacing) {
        self.spacing = spacing;
        self.update();
    }

    fn format_text(&self, content: String, spacing: Spacing) -> String {
        format!(
            "{}{}{}",
            self.icon
                .clone()
                .unwrap_or_else(|| spacing.to_string_leading()),
            content,
            spacing.to_string_trailing()
        )
    }

    fn update(&mut self) {
        let (key_bg, key_fg) = self.state.theme_keys(&self.shared_config.theme);

        self.inner.full_text =
            self.format_text(self.content.clone().unwrap_or_default(), self.spacing);
        self.inner.short_text = match &self.content_short {
            Some(text) => Some(self.format_text(text.clone(), self.spacing_short)),
            _ => None,
        };
        self.inner.background = key_bg;
        self.inner.color = key_fg;
    }
}

impl I3BarWidget for TextWidget {
    fn get_data(&self) -> I3BarBlock {
        self.inner.clone()
    }
}
