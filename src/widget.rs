use crate::config::SharedConfig;
use crate::errors::*;
use crate::formatting::{Format, Fragment, Values};
use crate::protocol::i3bar_block::I3BarBlock;
use serde::Deserialize;
use smart_default::SmartDefault;

#[derive(Debug, Clone, Default)]
pub struct Widget {
    pub state: State,
    source: Source,
}

impl Widget {
    pub fn new() -> Self {
        Self::default()
    }

    /*
     * Builders
     */

    pub fn with_text(mut self, text: String) -> Self {
        self.set_text(text);
        self
    }

    pub fn with_state(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    pub fn with_format(mut self, format: Format) -> Self {
        self.set_format(format);
        self
    }

    /*
     * Setters
     */

    pub fn set_text(&mut self, text: String) {
        if text.is_empty() {
            self.source = Source::None;
        } else {
            self.source = Source::Text(text);
        }
    }

    pub fn set_format(&mut self, format: Format) {
        match &mut self.source {
            Source::Format(old, _) => *old = format,
            _ => self.source = Source::Format(format, None),
        }
    }

    pub fn set_values(&mut self, new_values: Values) {
        if let Source::Format(_, values) = &mut self.source {
            *values = Some(new_values);
        }
    }

    pub fn intervals(&self) -> Vec<u64> {
        match &self.source {
            Source::Format(f, _) => f.intervals(),
            _ => Vec::new(),
        }
    }

    /// Construct `I3BarBlock` from this widget
    pub fn get_data(&self, shared_config: &SharedConfig, id: usize) -> Result<Vec<I3BarBlock>> {
        // Create a "template" block
        let (key_bg, key_fg) = shared_config.theme.get_colors(self.state);
        let (full, short) = self.source.render(shared_config)?;
        let mut template = I3BarBlock {
            instance: format!("{id}:"),
            background: key_bg,
            color: key_fg,
            ..I3BarBlock::default()
        };

        // Collect all the pieces into "parts"
        let mut parts = Vec::new();

        if full.is_empty() {
            return Ok(parts);
        }

        // If short text is available, it's necessary to hide all full blocks. `swaybar`/`i3bar`
        // will switch a block to "short mode" only if it's "short_text" is set to a non-empty
        // string "<span/>" is a non-empty string and it doesn't display anything. It's kinda hacky,
        // but it works.
        if !short.is_empty() {
            template.short_text = "<span/>".into();
        }

        parts.extend(full.into_iter().map(|w| {
            let mut data = template.clone();
            data.full_text = w.formated_text();
            if let Some(i) = &w.metadata.instance {
                data.instance.push_str(i);
            }
            data
        }));

        template.full_text = "<span/>".into();
        parts.extend(short.into_iter().map(|w| {
            let mut data = template.clone();
            data.short_text = w.formated_text();
            if let Some(i) = &w.metadata.instance {
                data.instance.push_str(i);
            }
            data
        }));

        Ok(parts)
    }
}

/// State of the widget. Affects the theming.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, SmartDefault)]
pub enum State {
    #[default]
    Idle,
    Info,
    Good,
    Warning,
    Critical,
}

/// The source of text for widget
#[derive(Debug, Clone, SmartDefault)]
enum Source {
    /// Collapsed widget (only icon will be displayed)
    #[default]
    None,
    /// Simple text
    Text(String),
    /// A format template
    Format(Format, Option<Values>),
}

impl Source {
    fn render(&self, config: &SharedConfig) -> Result<(Vec<Fragment>, Vec<Fragment>)> {
        match self {
            Self::Text(text) => Ok((vec![text.clone().into()], vec![])),
            Self::Format(format, Some(values)) => format.render(values, config),
            Self::None | Self::Format(_, None) => Ok((vec![], vec![])),
        }
    }
}
