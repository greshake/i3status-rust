use crate::config::SharedConfig;
use crate::errors::*;
use crate::formatting::{Format, Rendered, Values};
use crate::protocol::i3bar_block::I3BarBlock;
use serde::Deserialize;
use smartstring::alias::String;
use tokio::sync::mpsc;

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
#[derive(Debug)]
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
    fn render(&self) -> Result<(Vec<Rendered>, Vec<Rendered>)> {
        match self {
            Self::Text(text) => Ok((vec![text.clone().into()], vec![])),
            Self::TextWithShort(full, short) => {
                Ok((vec![full.clone().into()], vec![short.clone().into()]))
            }
            Self::Format(format, Some(values)) => format.render(values),
            Self::None | Self::Format(_, None) => Ok((vec![], vec![])),
        }
    }

    fn notify(&self, widget: &Widget) {
        if let Some(tx) = &widget.widget_updates_sender {
            let intervals = match self {
                Self::Format(f, _) => f.intervals(),
                _ => Vec::new(),
            };
            tx.send((widget.id, intervals)).unwrap();
        }
    }
}

#[derive(Debug)]
pub struct Widget {
    id: usize,
    pub icon: String,
    pub shared_config: SharedConfig,
    pub state: State,

    widget_updates_sender: Option<mpsc::UnboundedSender<(usize, Vec<u64>)>>,

    inner: I3BarBlock,
    source: Source,
    backup: Option<(Source, State)>,
}

impl Widget {
    pub fn new(
        id: usize,
        shared_config: SharedConfig,
        widget_updates_sender: Option<mpsc::UnboundedSender<(usize, Vec<u64>)>>,
    ) -> Self {
        let inner = I3BarBlock {
            name: Some(id.to_string()),
            ..I3BarBlock::default()
        };

        Widget {
            id,
            icon: String::new(),
            shared_config,
            state: State::Idle,

            widget_updates_sender,

            inner,
            source: Source::Text(String::new()),
            backup: None,
        }
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

    /*
     * Setters
     */

    pub fn set_text(&mut self, text: String) {
        if text.is_empty() {
            self.source = Source::None;
        } else {
            self.source = Source::Text(text);
        }
        self.source.notify(self);
    }

    pub fn set_texts(&mut self, short: String, full: String) {
        self.source = Source::TextWithShort(short, full);
        self.source.notify(self);
    }

    pub fn set_format(&mut self, format: Format) {
        match &mut self.source {
            Source::Format(old, _) => *old = format,
            _ => self.source = Source::Format(format, None),
        }
        self.source.notify(self);
    }

    pub fn set_values(&mut self, new_values: Values) {
        if let Source::Format(_, values) = &mut self.source {
            *values = Some(new_values);
        }
    }

    /*
     * Preserve / Restore
     */

    pub fn preserve(&mut self) {
        self.backup = Some((
            std::mem::replace(&mut self.source, Source::Text(String::new())),
            self.state,
        ));
        self.source.notify(self);
    }

    pub fn restore(&mut self) {
        if let Some(backup) = self.backup.take() {
            self.source = backup.0;
            self.state = backup.1;
        }
        self.source.notify(self);
    }

    /// Constuct `I3BarBlock` from this widget
    pub fn get_data(&self) -> Result<Vec<I3BarBlock>> {
        // Create a "template" block
        let mut template = self.inner.clone();
        let (key_bg, key_fg) = self.shared_config.theme.get_colors(self.state);
        let (full, short) = self.source.render()?;
        template.background = key_bg;
        template.color = key_fg;

        // Collect all the pieces into "parts"
        let mut parts = Vec::new();

        // Icon block
        if !self.icon.is_empty() {
            let mut data = template.clone();
            data.full_text = self.icon.clone().into();
            parts.push(data);
        }

        if full.is_empty() {
            return Ok(parts);
        }

        if self.icon.is_empty() {
            let mut padding = template.clone();
            padding.full_text = " ".into();
            parts.push(padding);
        }

        // If short text is available, it's necessary to hide all full blocks. `swaybar`/`i3bar`
        // will switch a block to "short mode" only if it's "short_text" is set to a non-empty
        // string "<span/>" is a non-empty string and it doesn't display anything. It's kinda hacky,
        // but it works.
        if !short.is_empty() {
            template.short_text = "<span/>".into();
        }

        let full_cnt = full.len();
        parts.extend(full.into_iter().enumerate().map(|(i, w)| {
            let mut data = template.clone();
            data.full_text = w.text.into();
            if i + 1 == full_cnt {
                data.full_text.push(' ');
            }
            data.instance = w.metadata.instance.map(|i| i.to_string());
            if let Some(state) = w.metadata.state {
                let (key_bg, key_fg) = self.shared_config.theme.get_colors(state);
                data.background = key_bg;
                data.color = key_fg;
            }
            data
        }));

        let short_cnt = short.len();
        template.full_text = "<span/>".into();
        parts.extend(short.into_iter().enumerate().map(|(i, w)| {
            let mut data = template.clone();
            data.short_text = w.text.into();
            if i + 1 == short_cnt {
                data.short_text.push(' ');
            }
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
