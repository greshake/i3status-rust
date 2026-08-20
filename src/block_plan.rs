//! Block output contracts.
//!
//! Every standard block prepares a [`BlockPlan`] from its configuration before
//! running. The plan declares each output variant the block can produce, the
//! effective format for that output (defaults and inheritance already
//! resolved), and the icon names each icon-valued placeholder can carry. A
//! block that renders plain runtime text rather than a format declares that
//! too, as an [`OutputKind::Text`] output.
//!
//! The runtime renders through [`OutputHandle`]s derived from the plan, which
//! install the effective format and can look up the icon names the plan
//! declares. Icons are checked in one place, when a widget is published (see
//! [`Widget::check_contract`][crate::widget::Widget]): every icon value it
//! carries must be one the output declared, however that value was built.
//!
//! The invariant is that everything a block puts on the bar comes from a
//! declared output: publishing a format or text widget without a handle of
//! the matching kind fails, a plan that declares nothing is rejected when it
//! is built, and [`Widget::set_format`][crate::widget::Widget] and
//! [`Widget::set_text`][crate::widget::Widget] void the contract rather than
//! quietly keeping it. A widget with no source at all renders nothing and is
//! exempt.
//!
//! Because the plan is prepared before the block runs, a block's icon and
//! format surface can also be inspected without running it, and what such an
//! inspection sees cannot drift from what the block actually renders.
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

/// The icon the bar draws on a restartable block error, declared by
/// [`error_plan`] and rendered by the error widget.
pub(crate) const RESTART_ICON: &str = "refresh";

/// Marks an error caused by a block breaking its own contract rather than
/// by the user's configuration. Every contract failure is prefixed with it,
/// so the two kinds of error can be told apart by their message.
pub(crate) const CONTRACT_BUG: &str = "block contract bug";

/// The icon names one placeholder of one output variant can carry.
#[derive(Debug, Clone)]
pub(crate) enum IconChoices {
    /// A closed set, fully known once the block's configuration is parsed.
    /// Includes configuration-derived names (e.g. `toggle`'s `icon_on`).
    Fixed(Vec<Cow<'static, str>>),
    /// Any icon name is permitted and resolves through the normal icon set
    /// and override rules at render time. Reserved for deliberately dynamic
    /// blocks (`custom`, `custom_dbus`); standard blocks declare closed sets.
    OpenResolvable,
}

impl IconChoices {
    pub(crate) fn fixed<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<Cow<'static, str>>,
    {
        Self::Fixed(names.into_iter().map(Into::into).collect())
    }

    pub(crate) fn one<S: Into<Cow<'static, str>>>(name: S) -> Self {
        Self::Fixed(vec![name.into()])
    }

    pub(crate) fn permits(&self, name: &str) -> bool {
        // An empty icon name renders as empty output (a runtime no-op), so
        // it is always permitted.
        name.is_empty()
            || match self {
                Self::Fixed(names) => names.iter().any(|n| n == name),
                Self::OpenResolvable => true,
            }
    }

    /// The single fixed name, when the set has exactly one element.
    pub(crate) fn single(&self) -> Option<&Cow<'static, str>> {
        match self {
            Self::Fixed(names) if names.len() == 1 => Some(&names[0]),
            _ => None,
        }
    }
}

/// What an output renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputKind {
    /// A format template: placeholders, and icons among them.
    Format,
    /// Plain text computed at runtime, with no placeholders and no icons
    /// (`menu`). The text itself cannot be known before the block runs, but
    /// declaring the output keeps the block inside the contract: it states
    /// that this is all the block renders.
    Text,
}

/// One output variant of a block: a named state, its effective format, and
/// the icons each icon-valued placeholder may carry in that state.
#[derive(Debug, Clone)]
pub(crate) struct OutputPlan {
    id: Cow<'static, str>,
    kind: OutputKind,
    format: Format,
    icons: Vec<(&'static str, IconChoices)>,
}

impl OutputPlan {
    pub(crate) fn new(id: impl Into<Cow<'static, str>>, format: Format) -> Self {
        Self {
            id: id.into(),
            kind: OutputKind::Format,
            format,
            icons: Vec::new(),
        }
    }

    /// An output that renders plain runtime text instead of a format.
    pub(crate) fn text(id: impl Into<Cow<'static, str>>) -> Self {
        Self {
            id: id.into(),
            kind: OutputKind::Text,
            format: Format::new(
                "".parse().expect("the empty template always parses"),
                "".parse().expect("the empty template always parses"),
            ),
            icons: Vec::new(),
        }
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn kind(&self) -> OutputKind {
        self.kind
    }

    /// The effective format of this output. A [`OutputKind::Text`] output
    /// renders no template, so its format is empty.
    pub(crate) fn format(&self) -> &Format {
        &self.format
    }

    /// Declare the icon choices for an icon-valued placeholder.
    /// Duplicate declarations are rejected when the plan is built.
    pub(crate) fn icon(mut self, placeholder: &'static str, choices: IconChoices) -> Self {
        self.icons.push((placeholder, choices));
        self
    }

    pub(crate) fn choices_for(&self, placeholder: &str) -> Option<&IconChoices> {
        self.icons
            .iter()
            .find(|(p, _)| *p == placeholder)
            .map(|(_, c)| c)
    }

    /// The icon-valued placeholders this output declares. The bar itself
    /// renders through [`OutputHandle`] and looks placeholders up by name, so
    /// nothing in the hot path needs this — it exists because a plan that can
    /// only be queried by a name you already know is not inspectable. It is
    /// what `--doctor` and the declaration tests walk.
    pub(crate) fn icon_placeholders(&self) -> impl Iterator<Item = (&'static str, &IconChoices)> {
        self.icons.iter().map(|(p, c)| (*p, c))
    }

    /// Every static `^icon_name` this output's format can render: both the
    /// full and short templates, and every alternative branch within them.
    ///
    /// These are the other half of the icon surface. `net` and `speedtest`
    /// spell their arrows and their ping icon straight into the format
    /// rather than passing icon values, so they declare no icon placeholder
    /// for them, and an inspector that only walked [`Self::icon_placeholders`]
    /// would conclude the block draws no icons at all.
    #[cfg(test)]
    pub(crate) fn static_icons(&self) -> Vec<&str> {
        use crate::formatting::template::{FormatTemplate, Token};

        fn walk<'a>(template: &'a FormatTemplate, found: &mut Vec<&'a str>) {
            for list in template.0.iter() {
                for token in &list.0 {
                    match token {
                        Token::Icon { name } => {
                            if !found.contains(&name.as_str()) {
                                found.push(name);
                            }
                        }
                        Token::Recursive(inner) => walk(inner, found),
                        Token::Text(_) | Token::Placeholder { .. } => (),
                    }
                }
            }
        }
        let mut found = Vec::new();
        walk(&self.format.full, &mut found);
        walk(&self.format.short, &mut found);
        found
    }
}

/// One [`OutputPlan`] per format the user configured, so a block that can
/// rotate through several formats declares each of them. `declare` adds the
/// icon choices, which are the same whichever format is on screen.
///
/// Use this when the block's config has a `MaybeMultiFormatConfig`: the
/// number of outputs is not known until the config is parsed. A block with a
/// single `format` has one output and builds it inline instead (see
/// `docker`). This returns a `Vec` rather than a finished plan because a
/// block can append outputs that are not format variants (see `net`).
pub(crate) fn format_outputs(
    formats: Vec<Format>,
    declare: impl Fn(OutputPlan) -> OutputPlan,
) -> Vec<OutputPlan> {
    formats
        .into_iter()
        .enumerate()
        .map(|(index, format)| declare(OutputPlan::new(format_id(index), format)))
        .collect()
}

/// The output id of the `index`th configured format.
pub(crate) fn format_id(index: usize) -> Cow<'static, str> {
    match index {
        0 => "format".into(),
        n => format!("format{}", n + 1).into(),
    }
}

/// Rotating over the outputs [`format_outputs`] produced, mirroring the
/// bar's own next/prev format actions.
pub(crate) struct FormatRotation {
    outputs: Vec<OutputHandle>,
    index: usize,
}

impl FormatRotation {
    /// Collects the plan's format outputs. A block may declare further
    /// outputs for states that are not format variants (net's `inactive`
    /// and `missing`); those are left alone.
    pub(crate) fn new(plan: &Arc<BlockPlan>) -> Result<Self> {
        let outputs: Vec<OutputHandle> = (0..)
            .map_while(|index| plan.output(&format_id(index)).ok())
            .collect();
        if outputs.is_empty() {
            return Err(Error::new(format!(
                "{CONTRACT_BUG}: no format outputs to rotate over"
            )));
        }
        Ok(Self { outputs, index: 0 })
    }

    pub(crate) fn current(&self) -> &OutputHandle {
        &self.outputs[self.index]
    }

    pub(crate) fn next(&mut self) {
        self.index = (self.index + 1) % self.outputs.len();
    }

    pub(crate) fn prev(&mut self) {
        self.index = (self.index + self.outputs.len() - 1) % self.outputs.len();
    }
}

/// The full prepared contract of one block instance. The validated fields
/// are immutable from outside this module (read-only iteration only), so
/// construction-time uniqueness is a permanent invariant.
/// Deliberately not `Default`: an empty plan is exactly what [`Self::new`]
/// rejects, and a derived `Default` would hand every block a way around it.
#[derive(Debug, Clone)]
pub(crate) struct BlockPlan {
    outputs: Vec<OutputPlan>,
}

impl BlockPlan {
    /// Read-only view of the declared output variants.
    pub(crate) fn outputs(&self) -> impl Iterator<Item = &OutputPlan> {
        self.outputs.iter()
    }

    /// Build a plan, rejecting ambiguous or incomplete metadata
    /// unconditionally (also in release builds): a plan that declares nothing,
    /// duplicate output ids, duplicate icon-placeholder declarations within an
    /// output, and icons on a text output are all contract bugs.
    pub(crate) fn new(outputs: Vec<OutputPlan>) -> Result<Arc<Self>> {
        // A block that declares no outputs would render whatever it liked
        // with nothing to check it against, which is the one hole the
        // contract exists to close.
        if outputs.is_empty() {
            return Err(Error::new(format!(
                "{CONTRACT_BUG}: the plan declares no outputs"
            )));
        }
        for (i, output) in outputs.iter().enumerate() {
            if output.kind == OutputKind::Text && !output.icons.is_empty() {
                return Err(Error::new(format!(
                    "{CONTRACT_BUG}: text output '{}' declares icons, which it \
                     cannot render",
                    output.id
                )));
            }
            if outputs[..i].iter().any(|o| o.id == output.id) {
                return Err(Error::new(format!(
                    "{CONTRACT_BUG}: duplicate output id '{}'",
                    output.id
                )));
            }
            for (j, (placeholder, _)) in output.icons.iter().enumerate() {
                if output.icons[..j].iter().any(|(p, _)| p == placeholder) {
                    return Err(Error::new(format!(
                        "{CONTRACT_BUG}: output '{}' declares '${placeholder}' twice",
                        output.id
                    )));
                }
            }
        }
        Ok(Arc::new(Self { outputs }))
    }

    /// Handle for the output variant named `id`.
    pub(crate) fn output(self: &Arc<Self>, id: &str) -> Result<OutputHandle> {
        let index = self
            .outputs
            .iter()
            .position(|o| o.id == id)
            .or_error(|| format!("{CONTRACT_BUG}: the plan has no output variant '{id}'"))?;
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
pub(crate) struct OutputHandle {
    plan: Arc<BlockPlan>,
    index: usize,
}

impl OutputHandle {
    pub(crate) fn output(&self) -> &OutputPlan {
        &self.plan.outputs[self.index]
    }

    pub(crate) fn id(&self) -> &str {
        self.output().id()
    }

    pub(crate) fn format(&self) -> &Format {
        self.output().format()
    }

    /// The one icon name this output declares for `placeholder`. Lets runtime
    /// code take the name from the plan instead of repeating the string.
    pub(crate) fn single_icon(&self, placeholder: &str) -> Result<Cow<'static, str>> {
        self.output()
            .choices_for(placeholder)
            .and_then(IconChoices::single)
            .cloned()
            .or_error(|| {
                format!(
                    "{CONTRACT_BUG}: output '{}' does not declare exactly one \
                     icon for '${placeholder}'",
                    self.id()
                )
            })
    }

    /// The single declared icon of `placeholder`, as a value.
    pub(crate) fn icon_value(&self, placeholder: &str) -> Result<Value> {
        let name = self.single_icon(placeholder)?;
        Ok(Value::icon(name))
    }

    /// A widget rendering this output: effective format installed, icon
    /// values checked against the declared choices. Publishing it fails if
    /// this output is a [`OutputKind::Text`] one.
    pub(crate) fn new_widget(&self) -> Widget {
        Widget::from_output(self)
    }

    /// A widget rendering `text` as this output. Publishing it fails unless
    /// the output was declared as [`OutputKind::Text`].
    pub(crate) fn new_text_widget(&self, text: String) -> Widget {
        Widget::text_from_output(self, text)
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
                        "{CONTRACT_BUG}: icon '{name}' set for placeholder '${key}' \
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
/// conditional `refresh` icon shown for restartable errors. The bar renders
/// every block error through these handles.
#[derive(Debug)]
pub(crate) struct ErrorOutputs {
    pub error: OutputHandle,
    pub fullscreen: OutputHandle,
}

pub(crate) fn error_outputs(
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
pub(crate) fn error_plan(
    error_format: Format,
    error_fullscreen_format: Format,
    restartable_possible: bool,
) -> Result<Arc<BlockPlan>> {
    let output = |id, format| {
        let output = OutputPlan::new(id, format);
        if restartable_possible {
            output.icon("restart_block_icon", IconChoices::one(RESTART_ICON))
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
        let values = map!("icon" => crate::formatting::value::Value::icon("net_vpn"));
        assert!(handle.icon_violations(&values).is_empty());
    }

    #[test]
    fn an_icon_built_outside_the_plan_is_still_caught() {
        // The published Value::icon constructor is unchecked by design;
        // the contract is enforced when the widget is published, so a
        // block that bypasses its handle does not get away with it.
        let plan = plan();
        let handle = plan.output("connected").unwrap();
        let mut widget = handle.new_widget();
        widget.set_values(map!("icon" => crate::formatting::value::Value::icon("net_wired")));
        assert!(widget.check_contract().is_err());
    }

    #[test]
    fn undeclared_icon_is_a_violation() {
        let plan = plan();
        let handle = plan.output("connected").unwrap();
        let values = map!("icon" => crate::formatting::value::Value::icon("net_wired"));
        let violations = handle.icon_violations(&values);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("net_wired"));
        assert!(violations[0].contains("'connected'"));
    }

    #[test]
    fn undeclared_placeholder_is_a_violation() {
        let plan = plan();
        let handle = plan.output("connected").unwrap();
        let values = map!("other" => crate::formatting::value::Value::icon("net_vpn"));
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
            "restart_block_icon" => crate::formatting::value::Value::icon("refresh"),
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

    #[test]
    fn a_plan_that_declares_nothing_is_rejected() {
        // `BlockPlan::new` is the only way to build a plan. A `Default` impl
        // (derived or written) would be a second, unchecked constructor
        // producing exactly the empty plan this rejects, so assert at compile
        // time that there is none: the two implementations below overlap if,
        // and only if, `BlockPlan: Default`.
        trait OnlyBuiltByNew {}
        impl<T: Default> OnlyBuiltByNew for T {}
        impl OnlyBuiltByNew for BlockPlan {}
        fn only_built_by_new<T: OnlyBuiltByNew>() {}
        only_built_by_new::<BlockPlan>();

        // Otherwise a block could satisfy the framework with an empty plan
        // and then render whatever it liked.
        let err = BlockPlan::new(Vec::new()).unwrap_err().to_string();
        assert!(err.contains(CONTRACT_BUG), "{err}");
        assert!(err.contains("no outputs"), "{err}");
    }

    #[test]
    fn a_text_output_cannot_declare_icons() {
        let err = BlockPlan::new(vec![
            OutputPlan::text("main").icon("icon", IconChoices::one("a")),
        ])
        .unwrap_err()
        .to_string();
        assert!(err.contains(CONTRACT_BUG), "{err}");
    }

    #[test]
    fn text_output_widgets_carry_the_contract() {
        let plan = BlockPlan::new(vec![OutputPlan::text("main")]).unwrap();
        let handle = plan.output("main").unwrap();
        assert_eq!(handle.output().kind(), OutputKind::Text);
        let widget = handle.new_text_widget("whatever the block computed".into());
        assert_eq!(widget.contract().unwrap().id(), "main");
        widget.check_contract().unwrap();
    }

    #[test]
    fn raw_text_replacement_invalidates_the_contract() {
        let plan = BlockPlan::new(vec![OutputPlan::text("main")]).unwrap();
        let handle = plan.output("main").unwrap();
        let mut widget = handle.new_text_widget("declared".into());
        // The same rule as `set_format`: text set outside the handle is not
        // what the output promised.
        widget.set_text("smuggled in".into());
        assert!(widget.contract().is_none());
        let err = widget.check_contract().unwrap_err().to_string();
        assert!(err.contains(CONTRACT_BUG), "{err}");
    }

    #[test]
    fn a_widget_must_render_the_kind_its_output_declared() {
        // Text handle, format widget.
        let text_plan = BlockPlan::new(vec![OutputPlan::text("main")]).unwrap();
        let err = text_plan
            .output("main")
            .unwrap()
            .new_widget()
            .check_contract()
            .unwrap_err()
            .to_string();
        assert!(err.contains(CONTRACT_BUG), "{err}");

        // Format handle, text widget.
        let plan = plan();
        let err = plan
            .output("connected")
            .unwrap()
            .new_text_widget("plain".into())
            .check_contract()
            .unwrap_err()
            .to_string();
        assert!(err.contains(CONTRACT_BUG), "{err}");
    }

    #[test]
    fn static_format_icons_are_enumerable() {
        // `^icon_*` tokens are the other half of the icon surface: they never
        // appear as declared placeholders, so a plan that could not report
        // them would understate what the block draws.
        let plan = BlockPlan::new(vec![OutputPlan::new(
            "main",
            FormatConfig::default()
                .with_defaults(
                    " ^icon_ping $ping {^icon_net_down $down|^icon_net_up} ",
                    " ^icon_ping ",
                )
                .unwrap(),
        )])
        .unwrap();
        let handle = plan.output("main").unwrap();
        // Both templates, both branches of the alternative, no duplicates.
        assert_eq!(
            handle.output().static_icons(),
            ["ping", "net_down", "net_up"]
        );
    }

    #[test]
    fn a_text_output_renders_no_static_icons() {
        let plan = BlockPlan::new(vec![OutputPlan::text("main")]).unwrap();
        assert!(
            plan.output("main")
                .unwrap()
                .output()
                .static_icons()
                .is_empty()
        );
    }

    #[test]
    fn a_contract_cannot_be_attached_to_a_format_the_output_never_declared() {
        // There is no setter for the contract alone: the only ways to get one
        // are the handle's constructors and `set_output`, all of which install
        // the output's own format at the same time. Anything else voids it.
        let plan = plan();
        let handle = plan.output("connected").unwrap();

        let mut widget = handle.new_widget();
        widget.set_format(format(" a totally different format "));
        assert!(widget.check_contract().is_err());

        widget.set_output(&handle);
        widget.check_contract().unwrap();
    }

    #[test]
    fn an_empty_widget_needs_no_output() {
        // A widget with no source renders nothing, so there is nothing to
        // hold it to: the bar uses this to collapse a block.
        Widget::new().check_contract().unwrap();
    }

    #[test]
    fn drift_between_declaration_and_runtime_is_marked_as_a_contract_bug() {
        let plan = plan();
        // A missing output id and a placeholder that is not declared with
        // exactly one icon are both programmer errors, not user ones.
        for err in [
            plan.output("never_declared").unwrap_err().to_string(),
            plan.output("disconnected")
                .unwrap()
                .single_icon("icon")
                .unwrap_err()
                .to_string(),
            plan.output("connected")
                .unwrap()
                .single_icon("never_declared")
                .unwrap_err()
                .to_string(),
        ] {
            assert!(err.contains(CONTRACT_BUG), "{err}");
        }
    }
}
