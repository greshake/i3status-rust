//! Block output contracts.
//!
//! Every standard block prepares a [`BlockPlan`] from its configuration before
//! running. The plan declares each output variant the block can produce, the
//! effective format for that output (defaults and inheritance already
//! resolved), and the icon names each icon-valued placeholder can carry.
//!
//! The runtime renders through [`OutputHandle`]s derived from the plan, which
//! install the effective format and check icon values against the declared
//! choices. `--doctor` analyzes the same plan. Because both sides consume the
//! same prepared data, doctor's static conclusions cannot drift from what the
//! block actually does at runtime.
//!
//! Declaring an icon does not resolve it. Resolution stays lazy: an icon is
//! looked up in the icon set only when a reachable format actually renders it,
//! so configurations that omit icons their formats never reference keep
//! working.

use std::borrow::Cow;
use std::sync::Arc;

use crate::errors::*;
use crate::formatting::Format;
use crate::formatting::value::{Value, ValueInner};
use crate::widget::Widget;

/// The icon names one placeholder of one output variant can carry.
#[derive(Debug, Clone)]
pub enum IconChoices {
    /// A closed set, fully known once the block's configuration is parsed.
    /// Includes configuration-derived names (e.g. `toggle`'s `icon_on`).
    Fixed(Vec<Cow<'static, str>>),
    /// Any icon name is permitted and resolves through the normal icon set
    /// and override rules at render time. Reserved for deliberately dynamic
    /// blocks (`custom`, `custom_dbus`); standard blocks declare closed sets.
    OpenResolvable,
}

impl IconChoices {
    pub fn fixed<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<Cow<'static, str>>,
    {
        Self::Fixed(names.into_iter().map(Into::into).collect())
    }

    pub fn one<S: Into<Cow<'static, str>>>(name: S) -> Self {
        Self::Fixed(vec![name.into()])
    }

    pub fn permits(&self, name: &str) -> bool {
        // An empty icon name renders as empty output (a runtime no-op), so
        // it is always permitted.
        name.is_empty()
            || match self {
                Self::Fixed(names) => names.iter().any(|n| n == name),
                Self::OpenResolvable => true,
            }
    }

    pub fn is_open(&self) -> bool {
        matches!(self, Self::OpenResolvable)
    }

    /// The single fixed name, when the set has exactly one element.
    pub fn single(&self) -> Option<&Cow<'static, str>> {
        match self {
            Self::Fixed(names) if names.len() == 1 => Some(&names[0]),
            _ => None,
        }
    }
}

/// Proof that an icon value was created through a plan handle after being
/// checked against the declared choices. Only this module can mint one, so
/// blocks cannot construct icon values that bypass their contract.
#[derive(Debug)]
pub struct IconToken {
    _private: (),
}

/// One output variant of a block: a named state, its effective format, the
/// icons each icon-valued placeholder may carry in that state, and the
/// values the block guarantees to provide on every render.
#[derive(Debug, Clone)]
pub struct OutputPlan {
    pub id: &'static str,
    pub format: Format,
    icons: Vec<(&'static str, IconChoices)>,
}

impl OutputPlan {
    pub fn new(id: &'static str, format: Format) -> Self {
        Self {
            id,
            format,
            icons: Vec::new(),
        }
    }

    /// Declare the icon choices for an icon-valued placeholder.
    /// Duplicate declarations are rejected when the plan is built.
    pub fn icon(mut self, placeholder: &'static str, choices: IconChoices) -> Self {
        self.icons.push((placeholder, choices));
        self
    }

    pub fn choices_for(&self, placeholder: &str) -> Option<&IconChoices> {
        self.icons
            .iter()
            .find(|(p, _)| *p == placeholder)
            .map(|(_, c)| c)
    }

    pub fn icon_placeholders(&self) -> impl Iterator<Item = (&'static str, &IconChoices)> {
        self.icons.iter().map(|(p, c)| (*p, c))
    }
}

/// The full prepared contract of one block instance.
#[derive(Debug, Clone, Default)]
pub struct BlockPlan {
    pub outputs: Vec<OutputPlan>,
}

impl BlockPlan {
    /// Build a plan, rejecting ambiguous metadata unconditionally (also in
    /// release builds): duplicate output ids and duplicate icon-placeholder
    /// declarations within an output are contract bugs.
    pub fn new(outputs: Vec<OutputPlan>) -> Result<Arc<Self>> {
        for (i, output) in outputs.iter().enumerate() {
            if outputs[..i].iter().any(|o| o.id == output.id) {
                return Err(Error::new(format!(
                    "block contract bug: duplicate output id '{}'",
                    output.id
                )));
            }
            for (j, (placeholder, _)) in output.icons.iter().enumerate() {
                if output.icons[..j].iter().any(|(p, _)| p == placeholder) {
                    return Err(Error::new(format!(
                        "block contract bug: output '{}' declares '${placeholder}' twice",
                        output.id
                    )));
                }
            }
        }
        Ok(Arc::new(Self { outputs }))
    }

    /// Handle for the output variant named `id`.
    pub fn output(self: &Arc<Self>, id: &str) -> Result<OutputHandle> {
        let index = self
            .outputs
            .iter()
            .position(|o| o.id == id)
            .or_error(|| format!("block plan has no output variant '{id}'"))?;
        Ok(OutputHandle {
            plan: self.clone(),
            index,
        })
    }
}

/// A handle to one declared output variant. Constructing a widget through the
/// handle installs the plan's effective format, so runtime code cannot select
/// a format the plan does not know about.
#[derive(Debug, Clone)]
pub struct OutputHandle {
    plan: Arc<BlockPlan>,
    index: usize,
}

impl OutputHandle {
    pub fn plan(&self) -> &Arc<BlockPlan> {
        &self.plan
    }

    pub fn output(&self) -> &OutputPlan {
        &self.plan.outputs[self.index]
    }

    pub fn id(&self) -> &'static str {
        self.output().id
    }

    pub fn format(&self) -> &Format {
        &self.output().format
    }

    /// The one icon name this output declares for `placeholder`. Lets runtime
    /// code take the name from the plan instead of repeating the string.
    pub fn single_icon(&self, placeholder: &str) -> Result<Cow<'static, str>> {
        self.output()
            .choices_for(placeholder)
            .and_then(IconChoices::single)
            .cloned()
            .or_error(|| {
                format!(
                    "output '{}' does not declare exactly one icon for '${placeholder}'",
                    self.id()
                )
            })
    }

    /// The single declared icon of `placeholder`, as a value.
    pub fn icon_value(&self, placeholder: &str) -> Result<Value> {
        let name = self.single_icon(placeholder)?;
        Ok(Value::icon(name, IconToken { _private: () }))
    }

    /// A declared icon of `placeholder` by name. For open (dynamic)
    /// placeholders any name is accepted; for fixed sets the name must be
    /// declared — anything else is an i3status-rs contract bug.
    pub fn named_icon_value<S>(&self, placeholder: &str, name: S) -> Result<Value>
    where
        S: Into<Cow<'static, str>>,
    {
        let name = name.into();
        self.check_declared(placeholder, &name)?;
        Ok(Value::icon(name, IconToken { _private: () }))
    }

    /// A declared progression icon of `placeholder`, with `value` in 0..=1.
    pub fn icon_progression<S>(&self, placeholder: &str, name: S, value: f64) -> Result<Value>
    where
        S: Into<Cow<'static, str>>,
    {
        let name = name.into();
        self.check_declared(placeholder, &name)?;
        Ok(Value::icon_progression(
            name,
            value,
            IconToken { _private: () },
        ))
    }

    /// Like [`Self::icon_progression`], with `value` first clamped to
    /// `low..=high` and rescaled to 0..=1.
    pub fn icon_progression_bound<S>(
        &self,
        placeholder: &str,
        name: S,
        value: f64,
        low: f64,
        high: f64,
    ) -> Result<Value>
    where
        S: Into<Cow<'static, str>>,
    {
        self.icon_progression(
            placeholder,
            name,
            (value.clamp(low, high) - low) / (high - low),
        )
    }

    fn check_declared(&self, placeholder: &str, name: &str) -> Result<()> {
        let permitted = self
            .output()
            .choices_for(placeholder)
            .is_some_and(|choices| choices.permits(name));
        if permitted {
            Ok(())
        } else {
            Err(Error::new(format!(
                "block contract bug: icon '{name}' is not declared for '${placeholder}' \
                 of output '{}'",
                self.id()
            )))
        }
    }

    /// A widget rendering this output: effective format installed, icon
    /// values checked against the declared choices.
    pub fn new_widget(&self) -> Widget {
        let mut widget = Widget::new().with_format(self.format().clone());
        widget.set_contract(self.clone());
        widget
    }

    /// Descriptions of every icon value in `values` that this output does not
    /// declare. Empty when the widget conforms to the contract.
    pub(crate) fn icon_violations(&self, values: &crate::formatting::Values) -> Vec<String> {
        let output = self.output();
        let mut violations = Vec::new();
        for (key, value) in values {
            if let ValueInner::Icon(name, _) = &value.inner {
                let ok = output
                    .choices_for(key)
                    .is_some_and(|choices| choices.permits(name));
                if !ok {
                    violations.push(format!(
                        "block contract bug: icon '{name}' set for placeholder '${key}' \
                         is not declared by output '{}'",
                        output.id
                    ));
                }
            }
        }
        violations
    }
}

/// The error outputs every block shares: the effective (global or per-block)
/// error formats with the standard error placeholders, including the
/// conditional `refresh` icon shown for restartable errors. The real bar
/// renders errors through these handles and doctor analyzes the same plan.
#[derive(Debug)]
pub struct ErrorOutputs {
    pub error: OutputHandle,
    pub fullscreen: OutputHandle,
}

pub fn error_outputs(
    error_format: Format,
    error_fullscreen_format: Format,
    restartable_possible: bool,
) -> ErrorOutputs {
    let plan = error_plan(error_format, error_fullscreen_format, restartable_possible)
        .expect("the error plan has statically unique ids");
    ErrorOutputs {
        error: OutputHandle {
            plan: plan.clone(),
            index: 0,
        },
        fullscreen: OutputHandle { plan, index: 1 },
    }
}

/// The plan behind [`error_outputs`]: output 0 is "error", output 1 is
/// "error_fullscreen". The conditional `refresh` restart icon is only
/// declared when the block can actually become restartable: without a
/// `max_retries` limit the bar retries forever and never renders the
/// restart button.
pub fn error_plan(
    error_format: Format,
    error_fullscreen_format: Format,
    restartable_possible: bool,
) -> Result<Arc<BlockPlan>> {
    let output = |id, format| {
        let output = OutputPlan::new(id, format);
        if restartable_possible {
            output.icon("restart_block_icon", IconChoices::one("refresh"))
        } else {
            output
        }
    };
    BlockPlan::new(vec![
        output("error", error_format),
        output("error_fullscreen", error_fullscreen_format),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatting::config::Config as FormatConfig;

    fn format(s: &str) -> Format {
        FormatConfig::default().with_default(s).unwrap()
    }

    fn plan() -> Arc<BlockPlan> {
        BlockPlan::new(vec![
            OutputPlan::new("connected", format(" VPN: $icon "))
                .icon("icon", IconChoices::one("net_vpn")),
            OutputPlan::new("disconnected", format(" VPN: $icon "))
                .icon("icon", IconChoices::fixed(["net_wired", "net_down"])),
        ])
        .unwrap()
    }

    #[test]
    fn output_lookup() {
        let plan = plan();
        assert_eq!(plan.output("connected").unwrap().id(), "connected");
        assert!(plan.output("bogus").is_err());
    }

    #[test]
    fn fixed_choices_permit_only_declared_names() {
        let plan = plan();
        let disconnected = plan.output("disconnected").unwrap();
        let choices = disconnected.output().choices_for("icon").unwrap();
        assert!(choices.permits("net_wired"));
        assert!(choices.permits("net_down"));
        assert!(!choices.permits("net_vpn"));
    }

    #[test]
    fn open_choices_permit_everything() {
        assert!(IconChoices::OpenResolvable.permits("anything_at_all"));
    }

    #[test]
    fn declared_icon_passes_validation() {
        let plan = plan();
        let handle = plan.output("connected").unwrap();
        let values = map!("icon" => crate::formatting::value::Value::test_icon("net_vpn"));
        assert!(handle.icon_violations(&values).is_empty());
    }

    #[test]
    fn undeclared_icon_is_a_violation() {
        let plan = plan();
        let handle = plan.output("connected").unwrap();
        let values = map!("icon" => crate::formatting::value::Value::test_icon("net_wired"));
        let violations = handle.icon_violations(&values);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("net_wired"));
        assert!(violations[0].contains("'connected'"));
    }

    #[test]
    fn undeclared_placeholder_is_a_violation() {
        let plan = plan();
        let handle = plan.output("connected").unwrap();
        let values = map!("other" => crate::formatting::value::Value::test_icon("net_vpn"));
        assert_eq!(handle.icon_violations(&values).len(), 1);
    }

    #[test]
    fn non_icon_values_are_ignored() {
        let plan = plan();
        let handle = plan.output("connected").unwrap();
        let values = map!("country" => crate::formatting::value::Value::text("ES".into()));
        assert!(handle.icon_violations(&values).is_empty());
    }

    #[test]
    fn error_outputs_declare_the_refresh_icon() {
        let outputs = error_outputs(
            format(" {$restart_block_icon |}{$short_error_message|X} "),
            format(" $full_error_message "),
            true,
        );
        assert_eq!(outputs.error.id(), "error");
        assert_eq!(outputs.fullscreen.id(), "error_fullscreen");
        assert_eq!(
            outputs.error.single_icon("restart_block_icon").unwrap(),
            "refresh"
        );
        let values = map!(
            "restart_block_icon" => crate::formatting::value::Value::test_icon("refresh"),
            "full_error_message" => crate::formatting::value::Value::text("boom".into()),
        );
        assert!(outputs.error.icon_violations(&values).is_empty());
        assert!(outputs.fullscreen.icon_violations(&values).is_empty());
    }

    #[test]
    fn duplicate_output_ids_are_rejected_unconditionally() {
        let result = BlockPlan::new(vec![
            OutputPlan::new("main", format(" a ")),
            OutputPlan::new("main", format(" b ")),
        ]);
        assert!(result.unwrap_err().to_string().contains("duplicate"));
    }

    #[test]
    fn duplicate_icon_declarations_are_rejected_unconditionally() {
        let result = BlockPlan::new(vec![
            OutputPlan::new("main", format(" $icon "))
                .icon("icon", IconChoices::one("a"))
                .icon("icon", IconChoices::one("b")),
        ]);
        assert!(result.unwrap_err().to_string().contains("twice"));
    }

    #[test]
    fn raw_format_replacement_invalidates_the_contract() {
        let plan = plan();
        let handle = plan.output("connected").unwrap();
        let mut widget = handle.new_widget();
        assert!(widget.check_contract().is_ok());
        // A raw set_format bypasses the plan: the contract is void and
        // publishing the widget must fail.
        widget.set_format(format(" ^icon_arbitrary "));
        assert!(widget.contract().is_none());
        assert!(widget.check_contract().is_err());
    }

    #[test]
    fn widget_from_handle_carries_contract() {
        let plan = plan();
        let handle = plan.output("connected").unwrap();
        let widget = handle.new_widget();
        assert_eq!(widget.contract().unwrap().id(), "connected");
    }
}
