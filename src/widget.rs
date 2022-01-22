use crate::config::SharedConfig;
use crate::errors::*;
use crate::formatting::{RunningFormat, Values};
use crate::protocol::i3bar_block::I3BarBlock;
use serde_derive::Deserialize;
use smartstring::alias::String;

/// Spacing around the widget
#[derive(Debug, Clone, Copy)]
pub enum Spacing {
    /// Add a leading and trailing space around the widget contents
    Normal,
    /// Hide both leading and trailing spaces when widget is hidden
    Hidden,
}

/// State of the widget. Affects the theming.
#[derive(Debug, Clone, Copy, Deserialize)]
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
    /// Simple text
    Text(String),
    /// Full and short texts
    TextWithShort(String, String),
    /// A format template
    Format(RunningFormat, Option<Values>),
}

impl Source {
    fn render(&self) -> Result<(String, Option<String>)> {
        match self {
            Source::Text(text) => Ok((text.clone(), None)),
            Source::TextWithShort(full, short) => Ok((full.clone(), Some(short.clone()))),
            Source::Format(format, Some(values)) => format.render(values),
            Source::Format(_, None) => Ok((String::new(), None)),
        }
    }
}

#[derive(Debug)]
pub struct Widget {
    instance: Option<usize>,
    pub icon: String,
    pub shared_config: SharedConfig,
    pub state: State,

    inner: I3BarBlock,
    source: Source,
    backup: Option<(Source, State)>,
}

impl Widget {
    pub fn new(id: usize, shared_config: SharedConfig) -> Self {
        let inner = I3BarBlock {
            name: Some(id.to_string()),
            ..I3BarBlock::default()
        };

        Widget {
            instance: None,
            icon: String::new(),
            shared_config,
            state: State::Idle,

            inner,
            source: Source::Text(String::new()),
            backup: None,
        }
    }

    /*
     * Builders
     */

    pub fn with_instance(mut self, instance: usize) -> Self {
        self.instance = Some(instance);
        self.inner.instance = Some(instance.to_string());
        self
    }

    pub fn with_icon_str(mut self, icon: String) -> Self {
        self.icon = icon;
        self
    }

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
        self.source = Source::Text(text);
    }

    pub fn set_texts(&mut self, short: String, full: String) {
        self.source = Source::TextWithShort(short, full);
    }

    pub fn set_format(&mut self, format: RunningFormat) {
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

    /*
     * Getters
     */

    pub fn get_instance(&self) -> Option<usize> {
        self.instance
    }

    /*
     * Preserve / Restore
     */

    pub fn preserve(&mut self) {
        self.backup = Some((
            std::mem::replace(&mut self.source, Source::Text(String::new())),
            self.state,
        ));
    }

    pub fn restore(&mut self) {
        if let Some(backup) = self.backup.take() {
            self.source = backup.0;
            self.state = backup.1;
        }
    }

    /// Constuct `I3BarBlock` from this widget
    pub fn get_data(&self) -> Result<I3BarBlock> {
        let mut data = self.inner.clone();

        let (key_bg, key_fg) = self.shared_config.theme.get_colors(self.state);
        data.background = key_bg;
        data.color = key_fg;

        let (full, short) = self.source.render()?;
        let full_spacing = if full.is_empty() {
            Spacing::Hidden
        } else {
            Spacing::Normal
        };
        let short_spacing = if short.as_ref().map(String::is_empty).unwrap_or(true) {
            Spacing::Hidden
        } else {
            Spacing::Normal
        };

        data.full_text = format!(
            "{}{}{}",
            match (self.icon.as_str(), full_spacing) {
                ("", Spacing::Normal) => " ",
                ("", Spacing::Hidden) => "",
                (icon, _) => icon,
            },
            full,
            match full_spacing {
                Spacing::Normal => " ",
                Spacing::Hidden => "",
            }
        );

        data.short_text = short.as_ref().map(|short_text| {
            format!(
                "{}{}{}",
                match (self.icon.as_str(), short_spacing) {
                    ("", Spacing::Normal) => " ",
                    ("", Spacing::Hidden) => "",
                    (icon, _) => icon,
                },
                short_text,
                match short_spacing {
                    Spacing::Normal => " ",
                    Spacing::Hidden => "",
                }
            )
        });

        Ok(data)
    }
}
