use crate::block_plan::{CONTRACT_BUG, OutputHandle, OutputKind};
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
    values: Values,
    /// The declared output variant this widget renders, when the block has
    /// been migrated to a prepared [`crate::block_plan::BlockPlan`].
    contract: Option<OutputHandle>,
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
        // Same reasoning as `set_format`: raw text is not what the output
        // handle promised, so the contract no longer describes this widget.
        // Text output is declared and rendered via `new_text_widget`.
        self.contract = None;
    }

    pub fn set_format(&mut self, format: Format) {
        self.source = Source::Format(format);
        // A raw format replacement invalidates whatever output contract the
        // widget carried: the plan no longer describes what will render.
        // Contracted widgets must switch outputs via `set_output` instead;
        // publishing this widget now fails the contract check.
        self.contract = None;
    }

    pub(crate) fn values(&self) -> &Values {
        &self.values
    }

    pub fn set_values(&mut self, new_values: Values) {
        self.values = new_values;
    }

    /// A widget rendering `output`'s declared format. Source and contract are
    /// set together and only here: there is deliberately no way to attach a
    /// handle to a widget whose source did not come from it, which would let
    /// an arbitrary format pass validation under a valid output's name.
    pub(crate) fn from_output(output: &OutputHandle) -> Self {
        let mut widget = Self::new();
        widget.set_output(output);
        widget
    }

    /// A widget rendering `text` as `output`, which must be a text output.
    /// See [`Self::from_output`] for why this is a constructor.
    pub(crate) fn text_from_output(output: &OutputHandle, text: String) -> Self {
        let mut widget = Self::new();
        widget.set_text(text);
        widget.contract = Some(output.clone());
        widget
    }

    /// Switch this widget to another declared output: installs that output's
    /// effective format and contract together.
    pub(crate) fn set_output(&mut self, output: &OutputHandle) {
        self.set_format(output.format().clone());
        self.contract = Some(output.clone());
    }

    #[cfg(test)]
    pub(crate) fn contract(&self) -> Option<&OutputHandle> {
        self.contract.as_ref()
    }

    /// Enforce the prepared contract at the point a block publishes this
    /// widget: everything that renders — a format or plain text alike — must
    /// come from an output handle of the matching kind, and every icon value
    /// must be declared. A failure here is an i3status-rs bug (the bar shows
    /// it as a block error), never a configuration problem.
    pub(crate) fn check_contract(&self) -> Result<()> {
        // A widget with no source renders nothing at all, so there is no
        // output to hold it to.
        if matches!(self.source, Source::None) {
            return Ok(());
        }
        let Some(handle) = &self.contract else {
            let kind = match self.source {
                Source::Format(_) => "format",
                _ => "text",
            };
            return Err(Error::new(format!(
                "{CONTRACT_BUG}: a {kind} widget was published without a \
                 prepared output handle"
            )));
        };
        let declared = handle.output().kind();
        let matches_source = match self.source {
            Source::Format(_) => declared == OutputKind::Format,
            Source::Text(_) => declared == OutputKind::Text,
            Source::None => true,
        };
        if !matches_source {
            return Err(Error::new(format!(
                "{CONTRACT_BUG}: output '{}' is declared as {declared:?} but \
                 the published widget renders {}",
                handle.id(),
                match self.source {
                    Source::Format(_) => "a format",
                    _ => "text",
                }
            )));
        }
        if let Some(violation) = self.contract_violations().into_iter().next() {
            return Err(Error::new(violation));
        }
        Ok(())
    }

    /// Icon values not declared by this widget's output contract.
    pub(crate) fn contract_violations(&self) -> Vec<String> {
        match &self.contract {
            Some(handle) => handle.icon_violations(&self.values),
            None => Vec::new(),
        }
    }

    pub fn intervals(&self) -> Vec<u64> {
        match &self.source {
            Source::Format(f) => f.intervals(),
            _ => Vec::new(),
        }
    }

    /// Construct `I3BarBlock` from this widget
    pub fn get_data(&self, shared_config: &SharedConfig, id: usize) -> Result<Vec<I3BarBlock>> {
        // Create a "template" block
        let (key_bg, key_fg) = shared_config.theme.get_colors(self.state);
        let (full, short) = self.source.render(shared_config, &self.values)?;
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
            data.full_text = w.formatted_text();
            if let Some(i) = &w.metadata.instance {
                data.instance.push_str(i);
            }
            data
        }));

        template.full_text = "<span/>".into();
        parts.extend(short.into_iter().map(|w| {
            let mut data = template.clone();
            data.short_text = w.formatted_text();
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
    #[serde(alias = "idle")]
    Idle,
    #[serde(alias = "info")]
    Info,
    #[serde(alias = "good")]
    Good,
    #[serde(alias = "warning")]
    Warning,
    #[serde(alias = "critical")]
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
    Format(Format),
}

impl Source {
    fn render(
        &self,
        config: &SharedConfig,
        values: &Values,
    ) -> Result<(Vec<Fragment>, Vec<Fragment>)> {
        match self {
            Self::Text(text) => Ok((vec![text.clone().into()], vec![])),
            Self::Format(format) => format.render(values, config),
            Self::None => Ok((vec![], vec![])),
        }
    }
}
