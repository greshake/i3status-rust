use crate::config::SharedConfig;
use crate::errors::*;
use crate::escape::CollectEscaped;
use crate::formatting::{Format, Fragment, Values};
use crate::protocol::i3bar_block::I3BarBlock;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Widget {
    pub shared_config: SharedConfig,
    pub state: State,
    id: usize,
    source: Source,
}

impl Widget {
    pub fn new(id: usize, shared_config: SharedConfig) -> Self {
        Widget {
            shared_config,
            state: State::Idle,
            id,
            source: Source::Text(String::new()),
        }
    }

    pub fn new_error(id: usize, shared_config: SharedConfig, error: &Error) -> Self {
        Self::new(id, shared_config)
            .with_text(error.to_string().chars().collect_pango())
            .with_state(State::Critical)
    }

    /*
     * Builders
     */

    pub fn with_text(mut self, text: String) -> Self {
        self.source = Source::Text(text);
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

    pub fn set_texts(&mut self, short: String, full: String) {
        self.source = Source::TextWithShort(short, full);
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

    /// Constuct `I3BarBlock` from this widget
    pub fn get_data(&self) -> Result<Vec<I3BarBlock>> {
        // Create a "template" block
        let (key_bg, key_fg) = self.shared_config.theme.get_colors(self.state);
        let (full, short) = self.source.render()?;
        let mut template = I3BarBlock {
            name: Some(self.id.to_string()),
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
            data.full_text = w.text;
            data.instance = w.metadata.instance.map(|i| i.to_string());
            if let Some(state) = w.metadata.state {
                let (key_bg, key_fg) = self.shared_config.theme.get_colors(state);
                data.background = key_bg;
                data.color = key_fg;
            }
            data
        }));

        template.full_text = "<span/>".into();
        parts.extend(short.into_iter().map(|w| {
            let mut data = template.clone();
            data.short_text = w.text;
            data.instance = w.metadata.instance.map(|i| i.to_string());
            if let Some(state) = w.metadata.state {
                let (key_bg, key_fg) = self.shared_config.theme.get_colors(state);
                data.background = key_bg;
                data.color = key_fg;
            }
            data
        }));

        Ok(parts)
    }
}

/// State of the widget. Affects the theming.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum State {
    Idle,
    Info,
    Good,
    Warning,
    Critical,
}

impl Default for State {
    fn default() -> Self {
        Self::Idle
    }
}

/// The source of text for widget
#[derive(Debug, Clone)]
enum Source {
    /// Collapsed widget (only icon will be displayed)
    None,
    /// Simple text
    Text(String),
    /// Full and short texts
    TextWithShort(String, String),
    /// A format template
    Format(Format, Option<Values>),
}

impl Source {
    fn render(&self) -> Result<(Vec<Fragment>, Vec<Fragment>)> {
        match self {
            Self::Text(text) => Ok((vec![text.clone().into()], vec![])),
            Self::TextWithShort(full, short) => {
                Ok((vec![full.clone().into()], vec![short.clone().into()]))
            }
            Self::Format(format, Some(values)) => format.render(values),
            Self::None | Self::Format(_, None) => Ok((vec![], vec![])),
        }
    }
}
