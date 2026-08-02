//! `--doctor`: diagnose configuration problems.
//!
//! The report is assessment-first: short status lines for what works, a
//! per-block live test, a per-glyph icon table, and a final Problems list
//! where every finding gets a one-line diagnosis and a matter-of-fact fix.
//!
//! Sections:
//! - Header: which config file, icon set file and bar font are in use (the
//!   bar font is auto-detected from a running i3/sway over IPC; `--font`
//!   overrides it).
//! - Blocks: every configured block is instantiated and run for one cycle
//!   (unless `--doctor-skip-live`), showing its rendered output or a
//!   diagnosed failure — including render errors, which are explained with
//!   the placeholders the block actually provided. Note that this performs
//!   real work: network requests, command execution, D-Bus calls.
//! - Icons: one row per glyph with its codepoint and the font that will
//!   actually draw it (via fontconfig). Rows drawn by fonts outside the
//!   bar's font list are highlighted; icons not used by any configured
//!   block are collapsed at the end. "Used by" combines what blocks
//!   requested during the live test with static analysis (state-dependent
//!   icons a block did not request this cycle are marked "may").
//!
//! Static analysis is driven by each block's prepared contract
//! ([`crate::block_plan::BlockPlan`]): the same effective formats and
//! declared icon choices the runtime renders through, so the analysis cannot
//! drift from block behavior. An icon is required only when it is reachable
//! from the instance's effective formats — declared icons whose placeholder
//! or output variant the configured formats never reference are reported as
//! unused, not as problems. Every block has a contract (enforced at compile
//! time); a block instance whose configuration fails to deserialize is
//! analyzed conservatively (nothing claimed unused) and the configuration
//! error is reported.
//! - Problems: numbered findings with concrete fixes.
//!
//! Doctor never aborts: missing tools or broken config sections are
//! reported (with install/fix instructions where known) and the rest of the
//! report still runs.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use std::time::Duration;
use unicode_width::UnicodeWidthStr;

use futures::StreamExt as _;
use tokio::sync::mpsc;

use crate::blocks::CommonApi;
use crate::config::{BlockConfigEntry, Config, SharedConfig};
use crate::errors::*;
use crate::formatting::template as format_template;
use crate::formatting::value::ValueInner;
use crate::geolocator::Geolocator;
use crate::icons::{Icon, Icons};
use crate::util;
use crate::widget::Widget;
use crate::{Request, RequestCmd};

/// How long a block gets to produce its first output.
const LIVE_TIMEOUT: Duration = Duration::from_secs(10);

struct Problem {
    diagnosis: String,
    fix: Option<String>,
}

struct Style {
    red: &'static str,
    reset: &'static str,
}

impl Style {
    fn detect() -> Self {
        if std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none() {
            Self {
                red: "\x1b[31m",
                reset: "\x1b[0m",
            }
        } else {
            Self { red: "", reset: "" }
        }
    }
}

/// Returns the number of problems found, for use as the exit status: 0 means
/// a clean bill of health.
pub fn run(config_arg: &str, font_arg: Option<&str>, skip_live: bool) -> usize {
    let style = Style::detect();
    let mut problems: Vec<Problem> = Vec::new();

    println!("i3status-rs doctor");
    println!();

    // === Config file ===
    let (config_path, trace) = resolve_file(config_arg, None);
    let Some(config_path) = config_path else {
        problems.push(Problem {
            diagnosis: format!("Configuration file {config_arg:?} not found. Searched:\n{trace}"),
            fix: Some(
                "Create ~/.config/i3status-rust/config.toml, or pass a path: i3status-rs --doctor <path>"
                    .into(),
            ),
        });
        print_problems(&problems, false, &style);
        return problems.len();
    };
    println!("Config:   {}", config_path.display());

    let raw: Option<toml::Value> = match std::fs::read_to_string(&config_path) {
        Err(err) => {
            problems.push(Problem {
                diagnosis: format!("Cannot read {}: {err}", config_path.display()),
                fix: None,
            });
            None
        }
        Ok(text) => match toml::from_str(&text) {
            Ok(value) => Some(value),
            Err(err) => {
                problems.push(Problem {
                    diagnosis: format!("Configuration is not valid TOML:\n{err}"),
                    fix: Some(
                        "Fix the syntax error above; nothing else can be checked until the file parses."
                            .into(),
                    ),
                });
                None
            }
        },
    };
    let Some(raw) = raw else {
        print_problems(&problems, false, &style);
        return problems.len();
    };

    // === Icon set ===
    let builtin = Icons::default().0;
    let (set_name, overrides_value) = icons_config(&raw, &mut problems);
    let (base_map, set_desc) = if set_name == "none" {
        (builtin.clone(), "built-in text icons".to_string())
    } else {
        let (file, trace) = resolve_file(set_name, Some("icons"));
        match file {
            None => {
                problems.push(Problem {
                    diagnosis: format!(
                        "Icon set {set_name:?} not found. Icon sets are plain TOML files looked \
                         up on disk, not built into the binary. Searched:\n{trace}"
                    ),
                    fix: Some(format!(
                        "Copy the set (e.g. files/icons/{set_name}.toml from the source tree) \
                         into ~/.config/i3status-rust/icons/, or use an absolute path in `icons = `."
                    )),
                });
                (
                    builtin.clone(),
                    "built-in text icons (fallback)".to_string(),
                )
            }
            Some(file) => match util::deserialize_toml_file::<HashMap<String, Icon>, _>(&file) {
                Err(err) => {
                    problems.push(Problem {
                        diagnosis: format!(
                            "Icon set file {} cannot be parsed: {err}",
                            file.display()
                        ),
                        fix: Some(
                            "Fix the file, or remove `icons = ` to use the built-in text icons."
                                .into(),
                        ),
                    });
                    (
                        builtin.clone(),
                        "built-in text icons (fallback)".to_string(),
                    )
                }
                Ok(map) => {
                    let file = file.display().to_string();
                    let desc = if set_name == file {
                        format!("{file} ({} names)", map.len())
                    } else {
                        format!("{set_name} ({file}, {} names)", map.len())
                    };
                    (map, desc)
                }
            },
        }
    };
    println!("Icon set: {set_desc}");

    let global_overrides: HashMap<String, Icon> = match overrides_value {
        None => HashMap::new(),
        Some(value) => match value.clone().try_into() {
            Ok(overrides) => overrides,
            Err(err) => {
                problems.push(Problem {
                    diagnosis: format!("[icons.overrides] is not a valid icon table: {err}"),
                    fix: None,
                });
                HashMap::new()
            }
        },
    };

    // === Bar font ===
    let detected = if font_arg.is_none() {
        detect_bar_font()
    } else {
        None
    };
    let font_pattern = font_arg.or(detected.as_ref().map(|d| d.font.as_str()));
    // X core fonts (XLFD, "-misc-fixed-...") bypass fontconfig entirely, so
    // the glyph analysis would be meaningless.
    let is_xlfd = font_pattern.is_some_and(|f| f.starts_with('-'));
    let mut font_check = if is_xlfd {
        None
    } else {
        FontCheck::new(font_pattern)
    };
    // Findings that depend on the bar font are only authoritative when the
    // font is known unambiguously: given via --font, or auto-detected without
    // ambiguity. Otherwise they are reported as notes, not problems.
    let font_authoritative =
        font_arg.is_some() || (detected.as_ref().is_some_and(|d| d.note.is_none()));
    // Environment limitations are notes, not problems: they say what doctor
    // could not check, not that the user's configuration is wrong, and must
    // not affect the exit code.
    match (&font_check, font_arg, &detected) {
        (None, ..) if is_xlfd => {
            println!("Bar font: {} (X core font)", font_pattern.unwrap_or(""));
            println!(
                "   note: XLFD fonts bypass fontconfig, so doctor cannot analyze which font\n   \
                 draws each glyph; the font check is skipped."
            );
        }
        (None, ..) => {
            println!("Bar font: (unchecked)");
            println!(
                "   note: `fc-match` is not available, so doctor cannot tell which font will\n   \
                 draw each icon glyph — the most common cause of wrong icons. Install the\n   \
                 fontconfig utilities (package `fontconfig` on most distros) and re-run."
            );
        }
        (Some(_), Some(font), _) => println!("Bar font: {font} (from --font)"),
        (Some(_), None, Some(d)) => {
            println!(
                "Bar font: {} (auto-detected via {}, {})",
                d.font, d.tool, d.bar_id
            );
            if let Some(note) = &d.note {
                println!("   note: {note}");
            }
        }
        (Some(check), None, None) => {
            println!("Bar font: fontconfig default ({:?})", check.base_family);
            println!(
                "   note: no --font given and no running i3/sway answered over IPC; glyph\n   \
                 providers below may not match your actual bar and are reported as notes,\n   \
                 not problems. Re-run with the `font` directive from the bar {{ }} section\n   \
                 of your i3/sway config: i3status-rs --doctor --font \"pango:...\""
            );
        }
    }
    // icons_format (global or per-block) may explicitly select a font via
    // pango markup (font_family='...'); those families are deliberate
    // choices, not system fallbacks.
    if let Some(check) = font_check.as_mut() {
        let mut declared = icons_format_families(&raw);
        for block in raw_block_names(&raw)
            .iter()
            .enumerate()
            .filter_map(|(i, _)| {
                raw.get("block")
                    .and_then(|b| b.as_array())
                    .and_then(|b| b.get(i))
            })
        {
            if let Some(f) = block.get("icons_format").and_then(|v| v.as_str()) {
                declared.extend(pango_font_families(f));
            }
        }
        for family in declared {
            check.add_configured_family(&family);
        }
    }
    if let Some(check) = &font_check {
        if !check.has_fc_list {
            println!(
                "   note: `fc-list` is not available, so glyphs that no installed font\n   \
                 provides cannot be detected."
            );
        }
        for (family, resolved) in &check.families {
            // Generic fontconfig aliases (monospace, sans-serif, ...) always
            // resolve to some concrete family; that is normal, not a missing
            // font.
            if is_generic_family(family) {
                continue;
            }
            let installed = if check.has_fc_list {
                family_installed(family)
            } else {
                // fc-list is unavailable; fall back to comparing the resolved
                // family names
                resolved
                    .as_ref()
                    .is_some_and(|r| r.split(',').any(|m| family_eq(m, family)))
            };
            if !installed {
                let diagnosis = format!(
                    "Font {family:?} is in the bar's font list but not installed{}.",
                    resolved
                        .as_ref()
                        .map(|r| format!(" (fontconfig silently uses {r:?} in its place)"))
                        .unwrap_or_default()
                );
                if font_authoritative {
                    problems.push(Problem {
                        diagnosis,
                        fix: Some(format!(
                            "Install {family:?}, or remove it from the bar's font directive."
                        )),
                    });
                } else {
                    println!("   note: {diagnosis} (guessed font; not counted as a problem)");
                }
            }
        }
    }
    println!();

    // === Blocks (live test) ===
    let block_names = raw_block_names(&raw);
    // Everything is attributed per block INSTANCE: two blocks of the same
    // type get distinct labels ("time#1", "time#2") so per-instance
    // icons_overrides are analyzed separately.
    let labels = instance_labels(&block_names);
    let mut used_now: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    // What is known about each block instance's ability to render each icon;
    // starts from static format analysis, refined by the live render.
    let mut icon_relevant: HashMap<String, IconRelevance> = HashMap::new();

    let parsed: Option<Config> = match util::deserialize_toml_file::<Config, _>(&config_path) {
        Ok(config) => Some(config),
        Err(err) => {
            problems.push(Problem {
                diagnosis: format!("Configuration does not validate: {err}"),
                fix: None,
            });
            None
        }
    };

    // may-use: the icons each block instance may request in some state,
    // attributed by label: only the icons reachable from the instance's
    // effective formats, per its prepared contract.
    let mut may_use: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Any configured block declares an open dynamic icon source reachable
    // from its formats (custom/custom_dbus with $icon).
    let mut dynamic_blocks = false;

    // Labels whose block configuration failed to deserialize: the static
    // problem below already covers them, so the live report must not
    // diagnose the same root cause again.
    let mut prepared_errors: HashSet<String> = HashSet::new();
    // Any block whose contract could not be resolved: static conclusions
    // are then incomplete and "unused" can no longer be proven.
    let mut has_unknown = false;

    for index in 0..block_names.len() {
        let label = labels[index].clone();

        // The prepared contract of this block instance.
        let plan = parsed
            .as_ref()
            .and_then(|config| config.blocks.get(index))
            .and_then(|entry| match entry.config.plan() {
                Ok(plan) => Some(plan),
                Err(err) => {
                    // The block's own configuration did not deserialize (the
                    // bar would show a configuration error in its place).
                    problems.push(Problem {
                        diagnosis: format!(
                            "Block `{label}` cannot be prepared from its configuration: {err}"
                        ),
                        fix: None,
                    });
                    prepared_errors.insert(label.clone());
                    None
                }
            });

        let static_rel = match &plan {
            Some(plan) => {
                let analysis = analyze_plan(plan);
                // Reachable direct ^icon_* tokens in effective formats
                // (including defaults) are certain usage.
                for icon in &analysis.direct {
                    used_now
                        .entry(icon.clone())
                        .or_default()
                        .insert(label.clone());
                }
                for icon in &analysis.required {
                    may_use.entry(icon.clone()).or_default().push(label.clone());
                }
                dynamic_blocks |= analysis.open;
                StaticAnalysis::Contract(analysis)
            }
            None => {
                // No plan (the whole config or this block's table is
                // invalid): stay conservative — nothing can be proven
                // unused, and dynamic usage cannot be ruled out for the
                // custom family.
                dynamic_blocks |= matches!(block_names[index].as_str(), "custom" | "custom_dbus");
                has_unknown = true;
                StaticAnalysis::Unknown
            }
        };
        // Errors render through the shared error-widget plan with the
        // block's effective error formats; whatever those formats reach
        // (typically the conditional `refresh` restart icon) is a latent
        // requirement of every block.
        let error_rel = parsed
            .as_ref()
            .and_then(|config| config.blocks.get(index).map(|entry| (config, entry)))
            .map(|(config, entry)| {
                let plan = crate::block_plan::error_plan(
                    entry
                        .common
                        .error_format
                        .with_default_config(&config.error_format),
                    entry
                        .common
                        .error_fullscreen_format
                        .with_default_config(&config.error_fullscreen_format),
                    // The restart button (and its refresh icon) can only
                    // appear when retries are limited; configuration-error
                    // blocks are never restartable.
                    entry.common.max_retries.is_some()
                        && !matches!(entry.config, crate::blocks::BlockConfig::Err(..)),
                );
                analyze_plan(&plan)
            });
        if let Some(error_rel) = &error_rel {
            for icon in &error_rel.required {
                let users = may_use.entry(icon.clone()).or_default();
                if !users.contains(&label) {
                    users.push(label.clone());
                }
            }
        }
        icon_relevant.insert(
            label.clone(),
            IconRelevance {
                static_rel,
                error_rel,
                live: None,
            },
        );
    }

    let analysis_closed = parsed.is_some() && !has_unknown;

    let mut live_ran = false;
    // (label, icon) pairs already diagnosed by a live render error, so the
    // icon table does not report the same root cause twice
    let mut live_reported: HashSet<(String, String)> = HashSet::new();
    match (parsed, skip_live) {
        (None, _) => println!("Blocks: skipped (configuration does not validate)\n"),
        (Some(_), true) => println!("Blocks: skipped (--doctor-skip-live)\n"),
        (Some(config), false) => {
            live_ran = true;
            println!(
                "Blocks (each run for one cycle; performs real requests/commands, {}s timeout)",
                LIVE_TIMEOUT.as_secs()
            );
            let reports = run_live(config_arg, config.blocks.len(), &mut problems);
            let tag_w = reports
                .iter()
                .map(|r| format!("[{}] {}", r.index + 1, r.name).len())
                .max()
                .unwrap_or(0);
            let out_w = reports
                .iter()
                .filter_map(|r| match &r.verdict {
                    LiveVerdict::Rendered { text, .. } => Some(text.chars().count() + 2),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            for report in &reports {
                let label = labels
                    .get(report.index)
                    .cloned()
                    .unwrap_or_else(|| report.name.clone());
                match &report.verdict {
                    LiveVerdict::Rendered {
                        icons,
                        provided_icons,
                        contract_violations,
                        ..
                    } => {
                        for violation in contract_violations {
                            problems.push(Problem {
                                diagnosis: format!(
                                    "`{label}` violated its own output contract: {violation}. \
                                     This is a bug in i3status-rs, not in your configuration."
                                ),
                                fix: Some(
                                    "Report it at https://github.com/greshake/i3status-rust/issues."
                                        .into(),
                                ),
                            });
                        }
                        let rendered: HashSet<String> = icons.iter().cloned().collect();
                        let rendered_keys: HashSet<String> = provided_icons
                            .iter()
                            .filter(|(_, name)| rendered.contains(name))
                            .map(|(key, _)| key.clone())
                            .collect();
                        let provided_keys: HashMap<String, String> = provided_icons
                            .iter()
                            .map(|(key, name)| (name.clone(), key.clone()))
                            .collect();
                        if let Some(relevance) = icon_relevant.get_mut(&label) {
                            relevance.live = Some(LiveRelevance {
                                rendered,
                                provided_keys,
                                rendered_keys,
                            });
                        }
                    }
                    LiveVerdict::Skipped(_) => {
                        // the bar would not spawn this block at all
                        // (if_command exited non-zero): nothing it could
                        // request — including its formats' direct ^icon refs
                        if let Some(relevance) = icon_relevant.get_mut(&label) {
                            relevance.static_rel = StaticAnalysis::none();
                            relevance.error_rel = None;
                            relevance.live = None;
                        }
                        for users in may_use.values_mut() {
                            users.retain(|user| user != &label);
                        }
                        may_use.retain(|_, users| !users.is_empty());
                        for users in used_now.values_mut() {
                            users.remove(&label);
                        }
                        used_now.retain(|_, users| !users.is_empty());
                    }
                    LiveVerdict::RenderError { error, .. } => {
                        if let Some(icon) = error
                            .split("Icon '")
                            .nth(1)
                            .and_then(|rest| rest.split('\'').next())
                        {
                            live_reported.insert((label.clone(), icon.to_string()));
                        }
                        // Rendering failed, possibly on an icon. The
                        // prepared contract already names exactly what can
                        // render — including whatever the render failed on —
                        // so the static analysis stays authoritative.
                    }
                    _ => (),
                }
                print_block_report(
                    report,
                    &label,
                    tag_w,
                    out_w,
                    &style,
                    &mut problems,
                    &mut used_now,
                    &prepared_errors,
                );
            }
            println!();
        }
    }

    // === Icons table ===
    let block_overrides: Vec<(String, HashMap<String, Icon>)> =
        raw_block_overrides(&raw, &mut problems)
            .into_iter()
            .map(|(index, overrides)| (labels[index].clone(), overrides))
            .collect();
    print_icon_table(IconTableInput {
        base_map: &base_map,
        global_overrides: &global_overrides,
        block_overrides: &block_overrides,
        builtin: &builtin,
        used_now: &used_now,
        may_use: &may_use,
        dynamic_blocks,
        analysis_closed,
        icon_relevant: &icon_relevant,
        live_reported: &live_reported,
        font_check: &mut font_check,
        font_authoritative,
        style: &style,
        problems: &mut problems,
    });

    print_problems(&problems, live_ran, &style);
    problems.len()
}

/// Font families explicitly selected by the global icons_format.
fn icons_format_families(raw: &toml::Value) -> Vec<String> {
    raw.get("icons_format")
        .and_then(|v| v.as_str())
        .map(pango_font_families)
        .unwrap_or_default()
}

/// Extract font families from pango markup attributes:
/// `font_family='X'`, `face="X"`, `font='X 12'`.
fn pango_font_families(markup: &str) -> Vec<String> {
    let mut families = Vec::new();
    for attr in ["font_family=", "face=", "font_desc=", "font="] {
        let mut search = markup;
        while let Some(pos) = search.find(attr) {
            search = &search[pos + attr.len()..];
            let Some(quote) = search.chars().next().filter(|c| *c == '\'' || *c == '"') else {
                continue;
            };
            let value = &search[1..];
            if let Some(end) = value.find(quote) {
                let family = strip_font_modifiers(&value[..end]);
                if !family.is_empty() {
                    families.push(family);
                }
            }
        }
    }
    families
}

fn icons_config<'a>(
    raw: &'a toml::Value,
    problems: &mut Vec<Problem>,
) -> (&'a str, Option<&'a toml::Value>) {
    match raw.get("icons") {
        Some(toml::Value::String(name)) => {
            problems.push(Problem {
                diagnosis: format!("`icons = {name:?}` at the top level is not valid."),
                fix: Some(format!(
                    "Use a section:\n        [icons]\n        icons = {name:?}"
                )),
            });
            (name.as_str(), None)
        }
        Some(toml::Value::Table(table)) => (
            table
                .get("icons")
                .and_then(|v| v.as_str())
                .unwrap_or("none"),
            table.get("overrides"),
        ),
        _ => ("none", None),
    }
}

/// Per-block-instance `icons_overrides` tables from the raw config, keyed by
/// the block's position in the config.
fn raw_block_overrides(
    raw: &toml::Value,
    problems: &mut Vec<Problem>,
) -> Vec<(usize, HashMap<String, Icon>)> {
    let Some(blocks) = raw.get("block").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let name = block
            .get("block")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let Some(value) = block.get("icons_overrides") else {
            continue;
        };
        match value.clone().try_into() {
            Ok(overrides) => out.push((index, overrides)),
            Err(err) => problems.push(Problem {
                diagnosis: format!("{name}: icons_overrides is not a valid icon table: {err}"),
                fix: None,
            }),
        }
    }
    out
}

/// A unique label per block instance: the type name alone when the type
/// appears once, "name#N" (1-based config position) otherwise.
fn instance_labels(block_names: &[String]) -> Vec<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for name in block_names {
        *counts.entry(name.as_str()).or_default() += 1;
    }
    block_names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            if counts.get(name.as_str()).copied().unwrap_or(0) > 1 {
                format!("{name}#{}", index + 1)
            } else {
                name.clone()
            }
        })
        .collect()
}

fn raw_block_names(raw: &toml::Value) -> Vec<String> {
    raw.get("block")
        .and_then(|v| v.as_array())
        .map(|blocks| {
            blocks
                .iter()
                .map(|b| {
                    b.get("block")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<unknown>")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Live block test
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize)]
enum LiveVerdict {
    Rendered {
        text: String,
        /// Icon names actually rendered (recorded during rendering, so only
        /// the format branch that succeeded counts).
        icons: Vec<String>,
        /// All icon values the block provided: placeholder key -> icon name.
        provided_icons: Vec<(String, String)>,
        /// Prepared-contract violations (icons set outside the declared
        /// choices): internal i3status-rust bugs, not config problems.
        #[serde(default)]
        contract_violations: Vec<String>,
    },
    RenderError {
        error: String,
        provided: Vec<String>,
        icons: Vec<String>,
    },
    BlockError(String),
    Panicked(String),
    Hidden,
    Finished,
    NoOutput,
    Skipped(String),
    /// The if_command could not run: the real bar fails at startup in this
    /// case, so this is a problem.
    IfCommandFailed(String),
    /// The if_command exceeded doctor's deadline. The real bar waits without
    /// a timeout, so this is inconclusive, not a config problem.
    IfCommandTimeout(String),
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BlockReport {
    index: usize,
    name: String,
    verdict: LiveVerdict,
}

enum FirstOutput {
    Widget(Box<Widget>),
    Error(Error),
    Hidden,
}

/// Extra time the parent grants a worker beyond the block's own deadline
/// before SIGKILLing its process group.
const WORKER_GRACE: Duration = Duration::from_secs(3);

/// Run every block in its own worker process (`--doctor-worker <index>`, this
/// same binary). Future cancellation cannot reap OS processes a block has
/// spawned (e.g. the custom block's command), so each worker gets its own
/// process group, which the parent SIGKILLs after collecting the result —
/// nothing can hang doctor or outlive it.
fn run_live(config_arg: &str, count: usize, problems: &mut Vec<Problem>) -> Vec<BlockReport> {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => {
            return vec![BlockReport {
                index: 0,
                name: "<worker>".into(),
                verdict: LiveVerdict::BlockError(format!("cannot find own executable: {err}")),
            }];
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            return vec![BlockReport {
                index: 0,
                name: "<runtime>".into(),
                verdict: LiveVerdict::BlockError(format!("failed to start async runtime: {err}")),
            }];
        }
    };

    // A block's subprocess can leave its worker's process group (setsid), so
    // the group SIGKILL alone cannot guarantee cleanup. Registering as a
    // child subreaper makes every orphaned descendant reparent to us instead
    // of init; after the workers are done we sweep and kill whatever is left.
    // SAFETY: plain prctl syscall
    let subreaper_ok = unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) } == 0;
    if !subreaper_ok {
        problems.push(Problem {
            diagnosis: "Cannot register as a child subreaper; processes spawned by blocks \
                        that detach from their process group may outlive doctor."
                .into(),
            fix: None,
        });
    }

    let reports = runtime.block_on(async {
        let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let workers = (0..count).map(|index| {
            let exe = exe.clone();
            let done = done.clone();
            async move {
                let report = run_worker_process(&exe, config_arg, index).await;
                done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                report
            }
        });
        let work = futures::future::join_all(workers);
        // Blocks with slow commands or network requests can take their full
        // deadline; show progress while the user waits (tty only, so piped
        // output stays clean).
        if std::io::stdout().is_terminal() {
            use std::io::Write as _;
            const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let mut ticker = tokio::time::interval(Duration::from_millis(120));
            let mut frame = 0usize;
            tokio::pin!(work);
            loop {
                tokio::select! {
                    reports = &mut work => {
                        print!("\r\x1b[K");
                        let _ = std::io::stdout().flush();
                        break reports;
                    }
                    _ = ticker.tick() => {
                        print!(
                            "\r{} testing blocks… {}/{count} done",
                            FRAMES[frame % FRAMES.len()],
                            done.load(std::sync::atomic::Ordering::Relaxed),
                        );
                        let _ = std::io::stdout().flush();
                        frame += 1;
                    }
                }
            }
        } else {
            work.await
        }
    });

    if subreaper_ok && let Err(leftover) = sweep_orphaned_children() {
        problems.push(Problem {
            diagnosis: format!(
                "Could not clean up all processes spawned by the block test: {leftover}"
            ),
            fix: Some("Check for leftover processes and kill them manually.".into()),
        });
    }
    reports
}

/// Kill every remaining child of this process. All legitimate children (the
/// workers) have already been reaped by this point, so anything left is an
/// escaped descendant of some block's subprocess tree, reparented to us by
/// the subreaper registration.
///
/// Each pass can uncover one more level of a nested chain (killing a parent
/// orphans its children, which then reparent to us), so this sweeps until a
/// pass finds nothing, bounded by a deadline rather than a pass count.
/// Returns Err with a description if processes remain at the deadline.
fn sweep_orphaned_children() -> Result<(), String> {
    let self_pid = std::process::id();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        // Reap any zombies first so they neither linger nor hide behind the
        // early return below.
        // SAFETY: plain syscall
        while unsafe { libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) } > 0 {}
        let mut found = 0usize;
        let dir = match std::fs::read_dir("/proc") {
            Ok(dir) => dir,
            Err(err) => return Err(format!("cannot scan /proc: {err}")),
        };
        for entry in dir.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            // The process may exit between the scan and the read; that is
            // fine, it is gone either way.
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                continue;
            };
            // ppid is the second field after the parenthesized comm
            let Some((_, rest)) = stat.rsplit_once(')') else {
                continue;
            };
            let mut fields = rest.split_whitespace();
            let state = fields.next();
            let Some(ppid) = fields.next().and_then(|p| p.parse::<u32>().ok()) else {
                continue;
            };
            if ppid == self_pid && state != Some("Z") {
                // SAFETY: plain syscall. "No such process" (already gone) is
                // fine; any other failure leaves the process for the next
                // pass or the deadline report.
                unsafe { libc::kill(pid as i32, libc::SIGKILL) };
                found += 1;
            }
        }
        if found == 0 {
            return Ok(());
        }
        if std::time::Instant::now() > deadline {
            return Err(format!(
                "{found} process(es) still alive after 5s of sweeping"
            ));
        }
        // Give the kills a moment: children of the killed processes reparent
        // to us and are caught by the next iteration.
        std::thread::sleep(Duration::from_millis(10));
        // Reap the zombies so they vanish from the next scan.
        // SAFETY: plain syscall
        while unsafe { libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) } > 0 {}
    }
}

async fn run_worker_process(exe: &Path, config_arg: &str, index: usize) -> BlockReport {
    let fail = |msg: String| BlockReport {
        index,
        name: format!("block #{}", index + 1),
        verdict: LiveVerdict::BlockError(msg),
    };

    let mut command = tokio::process::Command::new(exe);
    command
        .arg("--doctor-worker")
        .arg(index.to_string())
        .arg(config_arg)
        // In the real bar all custom_dbus blocks share one connection and
        // one well-known name; workers are separate concurrent processes,
        // so give each a unique name to avoid NameTaken races (also against
        // a bar that is currently running).
        .env("I3RS_DBUS_NAME", format!("doctor{index}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    let child = match command.spawn() {
        Ok(child) => child,
        Err(err) => return fail(format!("cannot start doctor worker: {err}")),
    };
    let pid = child.id();

    let result = tokio::time::timeout(LIVE_TIMEOUT + WORKER_GRACE, child.wait_with_output()).await;

    // Sweep the worker's whole process group unconditionally: this reaps any
    // subprocess a block spawned and left behind (the worker itself is
    // already gone in the normal case).
    if let Some(pid) = pid {
        // SAFETY: plain syscall; negative pid targets the process group
        unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    }

    match result {
        Err(_) => {
            // reap the killed worker so it does not linger as a zombie
            if let Some(pid) = pid {
                // SAFETY: plain syscall; the group was just SIGKILLed
                unsafe { libc::waitpid(pid as i32, std::ptr::null_mut(), 0) };
            }
            fail(format!(
                "doctor worker did not finish within {}s and was killed",
                (LIVE_TIMEOUT + WORKER_GRACE).as_secs()
            ))
        }
        Ok(Err(err)) => fail(format!("doctor worker failed: {err}")),
        Ok(Ok(output)) => match serde_json::from_slice::<BlockReport>(&output.stdout) {
            Ok(report) => report,
            Err(err) => fail(format!(
                "doctor worker produced no readable result ({err}); it may have crashed"
            )),
        },
    }
}

/// Entry point for `--doctor-worker <index>`: run one block's live test in
/// this process and print the result as JSON on stdout.
pub fn run_worker(config_arg: &str, index: usize) {
    let report = worker_report(config_arg, index);
    match serde_json::to_string(&report) {
        Ok(json) => println!("{json}"),
        Err(err) => eprintln!("doctor worker: cannot serialize report: {err}"),
    }
}

fn worker_report(config_arg: &str, index: usize) -> BlockReport {
    let fail = |msg: String| BlockReport {
        index,
        name: format!("block #{}", index + 1),
        verdict: LiveVerdict::BlockError(msg),
    };
    let Ok(Some(config_path)) = util::find_file(config_arg, None, Some("toml")) else {
        return fail("worker cannot find the configuration file".into());
    };
    let mut config: Config = match util::deserialize_toml_file(&config_path) {
        Ok(config) => config,
        Err(err) => return fail(format!("worker cannot parse the configuration: {err}")),
    };
    if index >= config.blocks.len() {
        return fail("worker got an out-of-range block index".into());
    }
    let entry = config.blocks.swap_remove(index);
    let shared = config.shared.clone();
    let geolocator = config.geolocator.clone();

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => return fail(format!("worker failed to start async runtime: {err}")),
    };
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, test_block(index, entry, shared, geolocator))
}

async fn test_block(
    index: usize,
    entry: BlockConfigEntry,
    mut shared: SharedConfig,
    geolocator: Arc<Geolocator>,
) -> BlockReport {
    let name = entry.config.name().to_string();
    // One deadline shared by everything the block test does: if_command and
    // the block's first output together get LIVE_TIMEOUT, not each.
    let deadline = tokio::time::Instant::now() + LIVE_TIMEOUT;

    if let Some(cmd) = &entry.common.if_command {
        // Subprocess cleanup is guaranteed by the worker's process group,
        // which the parent sweeps.
        let output = tokio::time::timeout_at(
            deadline,
            tokio::process::Command::new("sh")
                .args(["-c", cmd])
                .stdin(std::process::Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await;
        match output {
            Err(_) => {
                return BlockReport {
                    index,
                    name,
                    verdict: LiveVerdict::IfCommandTimeout(format!(
                        "if_command did not finish within {}s ({cmd})",
                        LIVE_TIMEOUT.as_secs()
                    )),
                };
            }
            Ok(Err(err)) => {
                return BlockReport {
                    index,
                    name,
                    verdict: LiveVerdict::IfCommandFailed(format!(
                        "if_command could not run: {err}"
                    )),
                };
            }
            Ok(Ok(output)) if !output.status.success() => {
                return BlockReport {
                    index,
                    name,
                    verdict: LiveVerdict::Skipped(format!(
                        "if_command exited non-zero ({cmd}) — the bar would not show this block"
                    )),
                };
            }
            Ok(Ok(_)) => (),
        }
    }

    // Apply per-block overrides the same way BarState::spawn_block does
    if let Some(icons_format) = entry.common.icons_format {
        shared.icons_format = Arc::new(icons_format);
    }
    if let Some(theme_overrides) = entry.common.theme_overrides
        && let Err(err) = Arc::make_mut(&mut shared.theme).apply_overrides(theme_overrides)
    {
        return BlockReport {
            index,
            name,
            verdict: LiveVerdict::BlockError(format!("invalid theme_overrides: {err}")),
        };
    }
    if let Some(icons_overrides) = entry.common.icons_overrides {
        Arc::make_mut(&mut shared.icons).apply_overrides(icons_overrides);
    }

    let (sender, mut receiver) = mpsc::unbounded_channel();
    let api = CommonApi {
        id: index,
        update_request: Arc::new(tokio::sync::Notify::new()),
        request_sender: sender,
        // effectively "never": doctor only watches the first output
        error_interval: Duration::from_secs(3600),
        geolocator,
        max_retries: Some(0),
    };

    let mut futures = futures::stream::FuturesUnordered::new();
    entry.config.spawn(api, &mut futures);
    let handle = tokio::task::spawn_local(async move {
        futures.next().await;
    });

    let first = tokio::time::timeout_at(deadline, async {
        loop {
            match receiver.recv().await {
                None => break None,
                Some(Request { cmd, .. }) => match cmd {
                    RequestCmd::SetWidget(widget) => {
                        break Some(FirstOutput::Widget(Box::new(widget)));
                    }
                    RequestCmd::SetError { error, .. } => break Some(FirstOutput::Error(error)),
                    RequestCmd::UnsetWidget => break Some(FirstOutput::Hidden),
                    _ => continue,
                },
            }
        }
    })
    .await;

    handle.abort();
    if let Err(err) = handle.await
        && err.is_panic()
    {
        return BlockReport {
            index,
            name,
            verdict: LiveVerdict::Panicked(join_error_message(err)),
        };
    }

    let verdict = match first {
        Err(_) => LiveVerdict::NoOutput,
        Ok(None) => LiveVerdict::Finished,
        Ok(Some(FirstOutput::Hidden)) => LiveVerdict::Hidden,
        Ok(Some(FirstOutput::Error(error))) => LiveVerdict::BlockError(unwrap_terminated(&error)),
        Ok(Some(FirstOutput::Widget(widget))) => render_widget(&widget, &shared, index),
    };

    BlockReport {
        index,
        name,
        verdict,
    }
}

fn render_widget(widget: &Widget, shared: &SharedConfig, index: usize) -> LiveVerdict {
    // Record which icons rendering actually consumes: syntactic inspection
    // of the format is not enough, because a fallback branch containing
    // $icon may never be evaluated.
    let recorder = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut shared = shared.clone();
    shared.icon_recorder = Some(recorder.clone());
    match widget.get_data(&shared, index) {
        Ok(segments) => {
            let mut icons: Vec<String> = recorder.lock().map(|r| r.clone()).unwrap_or_default();
            icons.sort_unstable();
            icons.dedup();
            LiveVerdict::Rendered {
                text: segments
                    .iter()
                    .map(|s| s.full_text.as_str())
                    .collect::<Vec<_>>()
                    .join(""),
                icons,
                provided_icons: provided_icon_values(widget),
                contract_violations: widget.contract_violations(),
            }
        }
        Err(error) => {
            let mut provided: Vec<String> = widget.values().keys().map(|k| k.to_string()).collect();
            provided.sort_unstable();
            LiveVerdict::RenderError {
                error: error.to_string(),
                provided,
                // rendering failed, possibly on an icon: count all of them
                icons: provided_icon_values(widget)
                    .into_iter()
                    .map(|(_, name)| name)
                    .collect(),
            }
        }
    }
}

/// All icon values a widget carries, as (placeholder key, icon name).
fn provided_icon_values(widget: &Widget) -> Vec<(String, String)> {
    let mut icons: Vec<(String, String)> = widget
        .values()
        .iter()
        .filter_map(|(key, value)| match &value.inner {
            ValueInner::Icon(name, _) => Some((key.to_string(), name.to_string())),
            _ => None,
        })
        .collect();
    icons.sort_unstable();
    icons.dedup();
    icons
}

/// The block restart machinery wraps the real error in "Block terminated".
fn unwrap_terminated(error: &Error) -> String {
    match (&error.message, &error.cause) {
        (Some(msg), Some(cause)) if msg == "Block terminated" => cause.to_string(),
        _ => error.to_string(),
    }
}

fn join_error_message(err: tokio::task::JoinError) -> String {
    match err.try_into_panic() {
        Ok(payload) => payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "panic with non-string payload".into()),
        Err(err) => err.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn print_block_report(
    report: &BlockReport,
    label: &str,
    tag_w: usize,
    out_w: usize,
    style: &Style,
    problems: &mut Vec<Problem>,
    used_now: &mut BTreeMap<String, HashSet<String>>,
    prepared_errors: &HashSet<String>,
) {
    let tag = format!("[{}] {}", report.index + 1, report.name);
    match &report.verdict {
        LiveVerdict::Rendered { text, icons, .. } => {
            let icon_note = if icons.is_empty() {
                String::new()
            } else {
                format!("   (icons: {})", icons.join(", "))
            };
            let cell = format!("\"{text}\"");
            // pad by chars: the cell may contain multi-byte glyphs
            let pad = out_w.saturating_sub(cell.chars().count());
            println!("{tag:<tag_w$} {cell}{:pad$}{icon_note}", "");
            for icon in icons {
                used_now
                    .entry(icon.clone())
                    .or_default()
                    .insert(label.to_string());
            }
        }
        LiveVerdict::RenderError {
            error,
            provided,
            icons,
        } => {
            println!(
                "{tag:<tag_w$} {}RENDER ERROR{}: {error}",
                style.red, style.reset
            );
            println!("    values the block provided: {}", provided.join(", "));
            for icon in icons {
                used_now
                    .entry(icon.clone())
                    .or_default()
                    .insert(label.to_string());
            }
            let fix = if error.contains("Placeholder") {
                Some(
                    "The format references a placeholder the block did not provide (it may be \
                     state-dependent). Make it optional with a fallback: \"{ $placeholder |}\"."
                        .to_string(),
                )
            } else if error.contains("Icon") {
                Some(
                    "Add the icon name to [icons.overrides] or use an icon set that defines it \
                     (see the icon table below)."
                        .to_string(),
                )
            } else {
                None
            };
            problems.push(Problem {
                diagnosis: format!("{}: render error: {error}", report.name),
                fix,
            });
        }
        LiveVerdict::BlockError(error) => {
            println!("{tag:<tag_w$} {}ERROR{}: {error}", style.red, style.reset);
            // A block that failed to prepare already has a static problem
            // with the same root cause; do not diagnose it twice.
            if !prepared_errors.contains(label) {
                problems.push(Problem {
                    diagnosis: format!("{}: {error}", report.name),
                    fix: suggest_fix(&report.name, error),
                });
            }
        }
        LiveVerdict::Panicked(msg) => {
            println!("{tag:<tag_w$} {}PANICKED{}: {msg}", style.red, style.reset);
            problems.push(Problem {
                diagnosis: format!(
                    "{}: panicked ({msg}). In the bar this would show a permanent error.",
                    report.name
                ),
                fix: Some(
                    "This is a bug in i3status-rs; please report it upstream with your block \
                     config."
                        .into(),
                ),
            });
        }
        LiveVerdict::Hidden => {
            println!("{tag:<tag_w$} (block hides itself — no data to show right now)");
        }
        LiveVerdict::Finished => println!("{tag:<tag_w$} (finished without producing output)"),
        LiveVerdict::NoOutput => println!(
            "{tag:<tag_w$} (no output within {}s — event-driven blocks may be waiting for an event; \
             probably fine)",
            LIVE_TIMEOUT.as_secs()
        ),
        LiveVerdict::Skipped(reason) => println!("{tag:<tag_w$} (skipped: {reason})"),
        LiveVerdict::IfCommandFailed(reason) => {
            println!(
                "{tag:<tag_w$} {}IF_COMMAND FAILED{}: {reason}",
                style.red, style.reset
            );
            problems.push(Problem {
                diagnosis: format!(
                    "{}: {reason} — the bar fails to start when if_command cannot run.",
                    report.name
                ),
                fix: Some("Fix or remove the if_command.".into()),
            });
        }
        LiveVerdict::IfCommandTimeout(reason) => {
            // The real bar waits for if_command without a timeout, so a slow
            // command is valid configuration; doctor just cannot evaluate it.
            println!("{tag:<tag_w$} (inconclusive: {reason}; the bar itself would wait for it)",);
        }
    }
}

fn suggest_fix(block: &str, error: &str) -> Option<String> {
    let lower = error.to_lowercase();
    if lower.contains("no such file") || lower.contains("program not found") {
        Some(format!(
            "A program the `{block}` block runs is not installed. Install it, or remove the block."
        ))
    } else if lower.contains("dbus") || lower.contains("d-bus") || lower.contains("org.freedesktop")
    {
        Some(format!(
            "A D-Bus service the `{block}` block needs is not available. Start the corresponding \
             daemon, or remove the block."
        ))
    } else if lower.contains("api key") || lower.contains("unauthorized") || lower.contains("401") {
        Some(format!(
            "Configure a valid API key for the `{block}` block."
        ))
    } else if lower.contains("rate limit") {
        Some("The service is rate limiting; wait, or reduce the update frequency.".into())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Icon table
// ---------------------------------------------------------------------------

fn provenance_desc(provenance: &Provenance) -> String {
    match provenance {
        Provenance::Base => "in the icon set".into(),
        Provenance::Global => "in [icons.overrides]".into(),
        Provenance::Local(block) => format!("in the `{block}` block's icons_overrides"),
    }
}

/// Where a block's icon actually comes from, in precedence order.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
enum Provenance {
    Base,
    Global,
    Local(String),
}

/// Authoritative static analysis derived from a block's prepared contract
/// (see [`crate::block_plan`]). Runtime and doctor consume the same effective
/// formats and declared icon choices, so this cannot drift from what the
/// block actually does.
#[derive(Default)]
struct PlanAnalysis {
    /// Icon names reachable from the effective formats: the declared choices
    /// of every reachable icon-valued placeholder, plus reachable direct
    /// `^icon_*` tokens. These are the block's actual requirements — a
    /// declared icon whose output formats never reference its placeholder is
    /// NOT here.
    required: HashSet<String>,
    /// Reachable direct `^icon_*` names (subset of `required`): certain
    /// usage, not state-dependent.
    direct: HashSet<String>,
    /// A reachable placeholder declares an open dynamic icon source
    /// (custom/custom_dbus): any name may be requested at runtime, so
    /// static coverage is inherently partial for this block.
    open: bool,
}

fn analyze_plan(plan: &crate::block_plan::BlockPlan) -> PlanAnalysis {
    use crate::block_plan::IconChoices;
    let mut analysis = PlanAnalysis::default();
    for output in &plan.outputs {
        for template in [
            output.format.full_template(),
            output.format.short_template(),
        ] {
            let mut placeholders = Vec::new();
            let mut icons = Vec::new();
            collect_reachable_compiled(template, &mut placeholders, &mut icons);
            // Empty icon names render as empty output (a runtime no-op),
            // so they are never requirements.
            for name in icons.into_iter().filter(|name| !name.is_empty()) {
                analysis.required.insert(name.clone());
                analysis.direct.insert(name);
            }
            for key in placeholders {
                match output.choices_for(&key) {
                    Some(IconChoices::Fixed(names)) => analysis.required.extend(
                        names
                            .iter()
                            .filter(|n| !n.is_empty())
                            .map(|n| n.to_string()),
                    ),
                    Some(IconChoices::OpenResolvable) => analysis.open = true,
                    None => (),
                }
            }
        }
    }
    analysis
}

/// [`collect_reachable`], but over the compiled template of an effective
/// [`crate::formatting::Format`] from a prepared contract (defaults and
/// inheritance already resolved).
fn collect_reachable_compiled(
    template: &format_template::FormatTemplate,
    placeholders: &mut Vec<String>,
    icon_refs: &mut Vec<String>,
) {
    for token_list in template.token_lists() {
        let mut branch_can_fail = false;
        for token in &token_list.0 {
            match token {
                format_template::Token::Placeholder { name, .. } => {
                    placeholders.push(name.clone());
                    branch_can_fail = true;
                }
                format_template::Token::Icon { name } => {
                    // A missing icon is a render error, not a branch-selection
                    // failure: it does not make the branch fall through.
                    icon_refs.push(name.clone());
                }
                format_template::Token::Recursive(rec) => {
                    collect_reachable_compiled(rec, placeholders, icon_refs);
                    branch_can_fail |= compiled_group_can_fail(rec);
                }
                format_template::Token::Text(_) => (),
            }
        }
        if !branch_can_fail {
            break;
        }
    }
}

/// A group fails only if every one of its branches can fail.
fn compiled_group_can_fail(template: &format_template::FormatTemplate) -> bool {
    template.token_lists().iter().all(|token_list| {
        token_list.0.iter().any(|token| match token {
            format_template::Token::Placeholder { .. } => true,
            format_template::Token::Recursive(rec) => compiled_group_can_fail(rec),
            format_template::Token::Icon { .. } | format_template::Token::Text(_) => false,
        })
    })
}

/// The static side of a block instance's icon analysis.
enum StaticAnalysis {
    /// From the block's prepared contract: exact.
    Contract(PlanAnalysis),
    /// No plan available (the configuration did not deserialize): nothing
    /// can be proven unused, so everything stays relevant.
    Unknown,
}

impl StaticAnalysis {
    /// Nothing is statically reachable (e.g. the bar would not spawn the
    /// block at all).
    fn none() -> Self {
        Self::Contract(PlanAnalysis::default())
    }

    fn is_relevant(&self, icon: &str) -> bool {
        match self {
            // Open (dynamic) sources deliberately do not make arbitrary
            // names relevant: only live evidence can confirm those.
            Self::Contract(analysis) => analysis.required.contains(icon),
            Self::Unknown => true,
        }
    }
}

/// Live render evidence (only ever *adds* relevance: a snapshot of one state
/// cannot rule out alternate states or formats).
struct LiveRelevance {
    /// Icon names the render consumed.
    rendered: HashSet<String>,
    /// Icon name -> the placeholder key it was provided under.
    provided_keys: HashMap<String, String>,
    /// Placeholder keys whose icons were consumed.
    rendered_keys: HashSet<String>,
}

impl LiveRelevance {
    fn is_relevant(&self, icon: &str) -> bool {
        if self.rendered.contains(icon) {
            return true;
        }
        match self.provided_keys.get(icon) {
            // Provided under a placeholder the render never used.
            Some(key) => self.rendered_keys.contains(key),
            // Unknown pathway (a state-dependent sibling like bat_charging):
            // the static analysis models these precisely (scopes, per-format
            // defaults), so live evidence stays strictly positive.
            None => false,
        }
    }
}

/// What is known about a block instance's ability to render a given icon.
/// Live evidence is combined with the static analysis, never substituted for
/// it: the current render proves what CAN happen, not what cannot.
struct IconRelevance {
    static_rel: StaticAnalysis,
    /// Analysis of the block's effective error/fullscreen-error formats
    /// (the shared error-widget plan every block renders errors through).
    error_rel: Option<PlanAnalysis>,
    live: Option<LiveRelevance>,
}

impl IconRelevance {
    fn is_relevant(&self, icon: &str) -> bool {
        self.static_rel.is_relevant(icon)
            || self
                .error_rel
                .as_ref()
                .is_some_and(|e| e.required.contains(icon))
            || self.live.as_ref().is_some_and(|l| l.is_relevant(icon))
    }
}

struct IconTableInput<'a> {
    base_map: &'a HashMap<String, Icon>,
    global_overrides: &'a HashMap<String, Icon>,
    block_overrides: &'a [(String, HashMap<String, Icon>)],
    builtin: &'a HashMap<String, Icon>,
    used_now: &'a BTreeMap<String, HashSet<String>>,
    /// Icon name -> labels of block instances that may request it in some
    /// state (computed in [`run`]: exact per-instance reachability for
    /// contract blocks, documented icon lists for legacy blocks).
    may_use: &'a BTreeMap<String, Vec<String>>,
    /// Whether any configured block can request arbitrary icon names at
    /// runtime (reachable open capability, e.g. custom/custom_dbus).
    dynamic_blocks: bool,
    /// Whether every block's contract was resolved (no configuration
    /// failures): only then can "unused" be proven statically.
    analysis_closed: bool,
    /// Per block label: what is known about its ability to render each icon.
    icon_relevant: &'a HashMap<String, IconRelevance>,
    /// (label, icon) pairs already diagnosed by a live render error.
    live_reported: &'a HashSet<(String, String)>,
    font_check: &'a mut Option<FontCheck>,
    font_authoritative: bool,
    style: &'a Style,
    problems: &'a mut Vec<Problem>,
}

fn print_icon_table(input: IconTableInput) {
    let IconTableInput {
        base_map,
        global_overrides,
        block_overrides,
        builtin,
        used_now,
        may_use,
        dynamic_blocks,
        analysis_closed,
        icon_relevant,
        live_reported,
        font_check,
        font_authoritative,
        style,
        problems,
    } = input;
    // Per-instance local overrides, keyed by instance label ("time#2"), so
    // two blocks of the same type are analyzed separately.
    let mut local: HashMap<&str, HashMap<&str, &Icon>> = HashMap::new();
    for (block, overrides) in block_overrides {
        let entry = local.entry(block.as_str()).or_default();
        for (name, icon) in overrides {
            entry.insert(name.as_str(), icon);
        }
    }

    // What a given block instance actually resolves an icon name to.
    let resolve = |icon: &str, block: &str| -> Option<(&Icon, Provenance)> {
        if let Some(found) = local.get(block).and_then(|o| o.get(icon)) {
            return Some((found, Provenance::Local(block.to_string())));
        }
        if let Some(found) = global_overrides.get(icon) {
            return Some((found, Provenance::Global));
        }
        base_map.get(icon).map(|found| (found, Provenance::Base))
    };

    for name in global_overrides.keys() {
        if !base_map.contains_key(name)
            && !builtin.contains_key(name)
            && !used_now.contains_key(name)
            && !may_use.contains_key(name.as_str())
        {
            let diagnosis = format!(
                "[icons.overrides] defines {name:?}, which no icon set defines and no \
                 configured block uses (typo?)."
            );
            if analysis_closed && !dynamic_blocks {
                // Every contract resolved and none is an open dynamic
                // source: the override is provably unreachable, with or
                // without the live test.
                problems.push(Problem {
                    diagnosis,
                    fix: None,
                });
            } else if dynamic_blocks {
                println!(
                    "note: {diagnosis} Not counted as a problem: a custom block could still \
                     request it at runtime."
                );
            } else {
                println!(
                    "note: {diagnosis} Not counted as a problem: some block configurations \
                     could not be analyzed."
                );
            }
        }
    }

    // usage: icon name -> [(block, is_may)]. A block's own icons_overrides
    // entry counts as usage: overriding it is explicit intent.
    let mut usage: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    for (icon, blocks) in used_now {
        for block in blocks {
            usage
                .entry(icon.clone())
                .or_default()
                .push((block.clone(), false));
        }
    }
    for (icon, blocks) in may_use {
        for block in blocks {
            let users = usage.entry((*icon).to_string()).or_default();
            if !users.iter().any(|(b, _)| b == block) {
                users.push(((*block).to_string(), true));
            }
        }
    }
    for (block, overrides) in block_overrides {
        for icon in overrides.keys() {
            let users = usage.entry(icon.clone()).or_default();
            if !users.iter().any(|(b, _)| b == block) {
                // an override shows intent, but only the block's formats
                // decide whether the icon can actually be requested
                users.push((block.clone(), true));
            }
        }
    }

    let mut used_rows: Vec<IconRow> = Vec::new();
    let mut unused_extra: Vec<String> = Vec::new();
    let mut fallback_rows = 0usize;
    let mut missing_rows = 0usize;

    for (icon_name, users) in &usage {
        // Group the using blocks by what the icon actually resolves to for
        // them: an override REPLACES the base glyph for its block, so only
        // glyphs some block really renders produce rows.
        let mut groups: BTreeMap<Provenance, Vec<String>> = BTreeMap::new();
        for (block, is_may) in users {
            // A block whose formats cannot render this icon cannot error on
            // it; skip latent (may) findings for it. Configuration-derived
            // names (toggle's icon_on="custom") appear in the contract under
            // their effective names, so no canonicalization is needed.
            let relevant = icon_relevant
                .get(block)
                .map(|r| r.is_relevant(icon_name))
                .unwrap_or(true);
            match resolve(icon_name, block) {
                // A block whose formats cannot render this icon will not put
                // its glyph on the bar either: keep it out of the table (and
                // out of the font findings), not just out of the missing
                // checks.
                _ if *is_may && !relevant => (),
                Some((icon, provenance)) => {
                    // An empty progression is stored but Icons::get returns
                    // None for it: at runtime it behaves like an undefined
                    // icon, not like a defined one.
                    if matches!(icon, Icon::Progression(steps) if steps.is_empty()) {
                        if (!*is_may || relevant)
                            && !live_reported.contains(&(block.clone(), icon_name.clone()))
                        {
                            problems.push(Problem {
                                diagnosis: format!(
                                    "Icon {icon_name:?} is defined as an empty progression ({}), \
                                     which the bar treats as undefined — the `{block}` block \
                                     will error when it requests it.",
                                    provenance_desc(&provenance)
                                ),
                                fix: Some("Give the progression at least one glyph.".into()),
                            });
                        }
                        continue;
                    }
                    let label = if *is_may {
                        format!("{block} (may)")
                    } else {
                        block.clone()
                    };
                    groups.entry(provenance).or_default().push(label);
                }
                // already diagnosed by the live render error for this block
                None if live_reported.contains(&(block.clone(), icon_name.clone())) => (),
                None => {
                    // Finding: this block would error when requesting it —
                    // unless it never actually can (only "may" usage counts
                    // as latent, both are real problems).
                    problems.push(Problem {
                        diagnosis: format!(
                            "Icon {icon_name:?} is not defined for the `{block}` block (not in \
                             the icon set, [icons.overrides], or the block's icons_overrides) — \
                             the block will error when it requests it."
                        ),
                        fix: Some(format!(
                            "Add `{icon_name}` to [icons.overrides] or to that block's \
                             icons_overrides."
                        )),
                    });
                }
            }
        }
        if groups.is_empty() {
            // every user was skipped as irrelevant: the icon is effectively
            // unused by the configuration
            if (base_map.contains_key(icon_name) || global_overrides.contains_key(icon_name))
                && !unused_extra.contains(icon_name)
            {
                unused_extra.push(icon_name.clone());
            }
            continue;
        }
        for (provenance, mut blocks) in groups {
            blocks.sort_unstable();
            let (icon, tag) = match &provenance {
                Provenance::Base => (base_map.get(icon_name), String::new()),
                Provenance::Global => (global_overrides.get(icon_name), " [override]".into()),
                Provenance::Local(block) => (
                    local
                        .get(block.as_str())
                        .and_then(|o| o.get(icon_name.as_str()))
                        .copied(),
                    format!(" [{block} override]"),
                ),
            };
            let Some(icon) = icon else { continue };
            push_icon_rows(
                &mut used_rows,
                icon_name,
                icon,
                &tag,
                &blocks.join(", "),
                font_check,
                &mut fallback_rows,
                &mut missing_rows,
            );
        }
    }

    // Names the USER defined (global or per-block overrides) that no
    // configured block can render. Names shipped by the icon set that the
    // configuration simply does not reference are not worth listing.
    let mut user_unused: Vec<&str> = Vec::new();
    for name in global_overrides
        .keys()
        .chain(block_overrides.iter().flat_map(|(_, o)| o.keys()))
    {
        if (!usage.contains_key(name) || unused_extra.contains(name))
            && !user_unused.contains(&name.as_str())
        {
            user_unused.push(name);
        }
    }
    user_unused.sort_unstable();

    println!("Icons referenced by your blocks");
    let name_w = column_width("Name", used_rows.iter().map(|r| r.name.as_str()));
    let codes_w = column_width("Code", used_rows.iter().map(|r| r.codes.as_str()));
    let provider_w = column_width(
        "Effectively provided by",
        used_rows.iter().map(|r| r.provider.as_str()),
    );
    // The glyph cell renders as "X" including the quotes; size it by
    // display width so multi-character text icons ("LO") stay aligned.
    let glyph_w = used_rows
        .iter()
        .map(|r| UnicodeWidthStr::width(r.glyph.as_str()) + 2)
        .chain(["Glyph".len()])
        .max()
        .unwrap_or(5);
    println!(
        "{:<name_w$}  {:<glyph_w$}  {:<codes_w$}  {:<provider_w$}  Used by",
        "Name", "Glyph", "Code", "Effectively provided by"
    );
    for row in &used_rows {
        let cell = format!("\"{}\"", row.glyph);
        let pad = glyph_w.saturating_sub(UnicodeWidthStr::width(cell.as_str()));
        let line = format!(
            "{:<name_w$}  {cell}{:pad$}  {:<codes_w$}  {:<provider_w$}  {}",
            row.name, "", row.codes, row.provider, row.used_by
        );
        if row.red {
            println!("{}{line}{}", style.red, style.reset);
        } else {
            println!("{line}");
        }
    }
    if fallback_rows > 0 || missing_rows > 0 || !user_unused.is_empty() {
        println!();
    }
    if fallback_rows > 0 {
        println!("* Glyph provider selected by the system because no font in your list has it.");
    }
    if missing_rows > 0 {
        println!("† No installed font has this glyph; it renders as an empty box.");
    }
    if !user_unused.is_empty() {
        println!(
            "Overrides you defined that no configured block can render: {}",
            user_unused.join(", ")
        );
    }
    println!();

    if fallback_rows > 0 {
        let diagnosis = format!(
            "{fallback_rows} icon glyph(s) will be drawn by fonts outside the bar's font \
             list (red rows above). Their appearance depends on which fonts happen to be \
             installed and can change with any font install or system update."
        );
        if font_authoritative {
            problems.push(Problem {
                diagnosis,
                fix: Some(
                    "Add a font that provides these glyphs to the bar's font directive \
                     (matching the icon set in use), and make sure it is installed."
                        .into(),
                ),
            });
        } else {
            println!("note: {diagnosis}");
            println!("note: the bar font was guessed, so this is not counted as a problem.");
            println!();
        }
    }
    if missing_rows > 0 {
        problems.push(Problem {
            diagnosis: format!(
                "{missing_rows} icon glyph(s) have no provider at all († rows above)."
            ),
            fix: Some(
                "Install a font that contains these codepoints, or override the icons with \
                 glyphs your fonts have."
                    .into(),
            ),
        });
    }
}

/// Expand an icon into one table row per glyph (progressions as "name k/n")
/// and classify each glyph's font provider.
#[allow(clippy::too_many_arguments)]
fn push_icon_rows(
    used_rows: &mut Vec<IconRow>,
    name: &str,
    icon: &Icon,
    tag: &str,
    used_by: &str,
    font_check: &mut Option<FontCheck>,
    fallback_rows: &mut usize,
    missing_rows: &mut usize,
) {
    let rows: Vec<(String, &str)> = match icon {
        Icon::Single(s) => vec![(name.to_string(), s.as_str())],
        Icon::Progression(steps) => steps
            .iter()
            .enumerate()
            .map(|(k, s)| (format!("{name} {}/{}", k + 1, steps.len()), s.as_str()))
            .collect(),
    };
    for (row_name, glyph) in rows {
        let codes = codepoints(glyph);
        let (provider, mark) = match font_check.as_mut() {
            None => ("?".to_string(), ""),
            Some(check) => match glyph_provider(check, glyph) {
                GlyphProvider::Known(family) => (first_family(&family), ""),
                GlyphProvider::Fallback(family) => {
                    *fallback_rows += 1;
                    (first_family(&family), " *")
                }
                GlyphProvider::Missing => {
                    *missing_rows += 1;
                    ("(none)".to_string(), " †")
                }
            },
        };
        used_rows.push(IconRow {
            name: row_name,
            glyph: glyph.to_string(),
            codes,
            provider: format!("{provider}{mark}{tag}"),
            used_by: used_by.to_string(),
            red: !mark.is_empty(),
        });
    }
}

struct IconRow {
    name: String,
    glyph: String,
    codes: String,
    provider: String,
    used_by: String,
    red: bool,
}

fn column_width<'a>(header: &str, values: impl Iterator<Item = &'a str>) -> usize {
    values
        .map(str::len)
        .chain([header.len()])
        .max()
        .unwrap_or(0)
}

fn codepoints(s: &str) -> String {
    let codes: Vec<String> = s
        .chars()
        .filter(|c| !c.is_ascii())
        .map(|c| format!("U+{:04X}", c as u32))
        .collect();
    if codes.is_empty() {
        "-".to_string()
    } else {
        codes.join(" ")
    }
}

enum GlyphProvider {
    /// Primary or configured fallback family: expected, not flagged.
    Known(String),
    Fallback(String),
    Missing,
}

/// fc-match reports family lists like "Font Awesome 5 Free,Font Awesome 5
/// Free Solid"; the first member is enough for display.
fn first_family(family: &str) -> String {
    family.split(',').next().unwrap_or(family).to_string()
}

fn glyph_provider(check: &mut FontCheck, glyph: &str) -> GlyphProvider {
    let mut result = GlyphProvider::Known(check.base_family.clone());
    for c in glyph.chars() {
        if c.is_ascii() {
            continue;
        }
        match check.check(c) {
            GlyphFont::Base => (),
            GlyphFont::Configured(family) => {
                if matches!(result, GlyphProvider::Known(_)) {
                    result = GlyphProvider::Known(family.clone());
                }
            }
            GlyphFont::Fallback(family) => {
                if !matches!(result, GlyphProvider::Missing) {
                    result = GlyphProvider::Fallback(family.clone());
                }
            }
            GlyphFont::Missing => result = GlyphProvider::Missing,
        }
    }
    result
}

fn print_problems(problems: &[Problem], live_ran: bool, style: &Style) {
    if problems.is_empty() {
        if live_ran {
            println!("Problems: none found");
        } else {
            println!(
                "Problems: none statically provable (the live block test did not run; \
                 runtime behavior — commands, services, dynamic icon names — was not \
                 checked)"
            );
        }
        return;
    }
    println!("{}Problems ({}){}", style.red, problems.len(), style.reset);
    for (i, problem) in problems.iter().enumerate() {
        println!("{}. {}", i + 1, problem.diagnosis);
        match &problem.fix {
            Some(fix) => println!("   Fix: {fix}"),
            None => println!("   (no fix suggestion)"),
        }
    }
}

// ---------------------------------------------------------------------------
// File resolution
// ---------------------------------------------------------------------------

/// Returns the winning path and a human-readable trace of every candidate.
fn resolve_file(file: &str, subdir: Option<&str>) -> (Option<PathBuf>, String) {
    let mut found: Option<PathBuf> = None;
    let mut trace = String::new();
    if !Path::new(file).is_absolute() {
        trace.push_str("   (not an absolute path; standard locations searched in order)\n");
    }
    for candidate in util::file_candidates(file, subdir, Some("toml")) {
        let line = match (candidate.try_exists(), &found) {
            (Ok(true), None) => {
                let line = format!("   ✓ {} ← using this\n", candidate.display());
                found = Some(candidate);
                line
            }
            (Ok(true), Some(_)) => format!("   ✓ {} (shadowed)\n", candidate.display()),
            (Ok(false), _) => format!("   ✗ {}\n", candidate.display()),
            (Err(err), _) => format!("   ? {} (could not check: {err})\n", candidate.display()),
        };
        trace.push_str(&line);
    }
    (found, trace.trim_end().to_string())
}

// ---------------------------------------------------------------------------
// Fonts (fc-match / fc-list / bar IPC)
// ---------------------------------------------------------------------------

/// How a single character gets rendered, according to fontconfig.
enum GlyphFont {
    /// Rendered by the primary (first) configured family.
    Base,
    /// Rendered by one of the other configured fallback families.
    Configured(String),
    /// No configured family has it; fontconfig substitutes this family.
    Fallback(String),
    /// No installed font provides it: renders as an empty box.
    Missing,
}

/// Asks fontconfig (via `fc-match`/`fc-list`) which font actually renders
/// each glyph. This happens in the bar, outside of i3status-rs, and is the
/// usual source of "my icon was silently replaced by a different symbol":
/// when no configured font has a codepoint, fontconfig substitutes another
/// installed font without telling anyone.
struct FontCheck {
    /// All configured families joined for fc-match queries.
    pattern: String,
    /// Resolved family of the whole pattern (what renders plain text).
    base_family: String,
    /// Each configured family with what fc-match resolves it to; a family
    /// that resolves to something else entirely is not installed.
    families: Vec<(String, Option<String>)>,
    /// Whether `fc-list` works; without it "no font provides this glyph"
    /// cannot be detected and is never claimed.
    has_fc_list: bool,
    cache: HashMap<char, GlyphFont>,
}

impl FontCheck {
    fn new(font_arg: Option<&str>) -> Option<Self> {
        let families = parse_font_directive(font_arg.unwrap_or(""));
        let pattern = families.join(",");
        let base_family = fc_match(&pattern, None)?;
        let families = families
            .into_iter()
            .map(|family| {
                let resolved = fc_match(&family, None);
                (family, resolved)
            })
            .collect();
        let has_fc_list = std::process::Command::new("fc-list")
            .arg("--version")
            .output()
            .is_ok_and(|out| out.status.success());
        Some(Self {
            pattern,
            base_family,
            families,
            has_fc_list,
            cache: HashMap::new(),
        })
    }

    /// Register an additional deliberately-selected family (e.g. from
    /// icons_format pango markup) so its glyphs count as configured.
    fn add_configured_family(&mut self, family: &str) {
        if self
            .families
            .iter()
            .any(|(name, _)| family_eq(name, family))
        {
            return;
        }
        let resolved = fc_match(family, None);
        self.families.push((family.to_string(), resolved));
        // family preferences changed: previously classified glyphs may now
        // be configured rather than fallback
        self.cache.clear();
    }

    fn check(&mut self, c: char) -> &GlyphFont {
        self.cache.entry(c).or_insert_with(|| {
            let charset = format!("{:x}", c as u32);
            if self.has_fc_list && !fc_list_provides(&charset) {
                return GlyphFont::Missing;
            }
            match fc_match(&self.pattern, Some(&charset)) {
                Some(family) if family_eq(&family, &self.base_family) => GlyphFont::Base,
                Some(family) => {
                    // fc-match prints a comma-separated family list; if any
                    // member is one of the configured families — by name, or
                    // by what the configured family canonically resolves to
                    // (generic aliases like "monospace" resolve to a concrete
                    // family) — this is the configured fallback doing its
                    // job, not a surprise.
                    let members: Vec<&str> = family.split(',').collect();
                    match self.families.iter().find(|(name, resolved)| {
                        members.iter().any(|m| family_eq(m, name))
                            || resolved.as_ref().is_some_and(|r| {
                                r.split(',')
                                    .any(|rm| members.iter().any(|m| family_eq(m, rm)))
                            })
                    }) {
                        Some((name, _)) => GlyphFont::Configured(name.clone()),
                        None => {
                            // The bar-pattern match ignores families that are
                            // selected out-of-band (icons_format pango
                            // markup): probe each configured family directly —
                            // if it provides the glyph itself, rendering will
                            // use it deliberately.
                            let direct = self.families.iter().find(|(name, _)| {
                                fc_match(name, Some(&charset)).is_some_and(|resolved| {
                                    resolved.split(',').any(|m| family_eq(m, name))
                                })
                            });
                            match direct {
                                Some((name, _)) => GlyphFont::Configured(name.clone()),
                                None => GlyphFont::Fallback(family),
                            }
                        }
                    }
                }
                None => GlyphFont::Missing,
            }
        })
    }
}

/// Fontconfig compares family names ignoring case and blanks
/// (FcStrCmpIgnoreBlanksAndCase); do the same.
fn family_eq(a: &str, b: &str) -> bool {
    let mut a = a.chars().filter(|c| !c.is_whitespace());
    let mut b = b.chars().filter(|c| !c.is_whitespace());
    loop {
        match (a.next(), b.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if x.eq_ignore_ascii_case(&y) => (),
            _ => return false,
        }
    }
}

/// Whether a family is installed, asked of fontconfig itself: `fc-list` with
/// a family pattern lists only fonts whose family matches under fontconfig's
/// own normalization (so "DejaVuSansMono" finds "DejaVu Sans Mono").
fn family_installed(family: &str) -> bool {
    std::process::Command::new("fc-list")
        .arg(family)
        .arg("family")
        .output()
        .is_ok_and(|out| {
            out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().is_empty()
        })
}

/// Fontconfig's generic aliases: asking for these by design resolves to some
/// installed concrete family.
fn is_generic_family(family: &str) -> bool {
    matches!(
        family.to_lowercase().as_str(),
        "monospace"
            | "mono"
            | "sans-serif"
            | "sans"
            | "serif"
            | "cursive"
            | "fantasy"
            | "system-ui"
            | "emoji"
            | "math"
    )
}

fn fc_match(pattern: &str, charset: Option<&str>) -> Option<String> {
    let mut pattern = pattern.to_string();
    if let Some(charset) = charset {
        pattern.push_str(&format!(":charset={charset}"));
    }
    let out = std::process::Command::new("fc-match")
        .arg("--format=%{family}")
        .arg(&pattern)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let family = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!family.is_empty()).then_some(family)
}

fn fc_list_provides(charset: &str) -> bool {
    std::process::Command::new("fc-list")
        .arg(format!(":charset={charset}"))
        .arg("family")
        .output()
        .is_ok_and(|out| out.status.success() && !out.stdout.is_empty())
}

struct DetectedFont {
    tool: &'static str,
    bar_id: String,
    font: String,
    /// Ambiguity warning when several bars with different fonts exist.
    note: Option<String>,
}

/// Ask the running i3/sway over IPC which font the bar is configured with,
/// so `--font` is only needed when there is no live bar to ask.
///
/// With several bars, prefer the one whose status_command runs i3status-rs;
/// if the remaining candidates disagree on the font, report the ambiguity.
fn detect_bar_font() -> Option<DetectedFont> {
    for tool in ["swaymsg", "i3-msg"] {
        let Ok(out) = std::process::Command::new(tool)
            .args(["-t", "get_bar_config"])
            .output()
        else {
            continue;
        };
        if !out.status.success() {
            continue;
        }
        let Ok(bar_ids) = serde_json::from_slice::<Vec<String>>(&out.stdout) else {
            continue;
        };
        // (bar_id, font, status_command)
        let mut bars: Vec<(String, String, String)> = Vec::new();
        for bar_id in bar_ids {
            let Ok(out) = std::process::Command::new(tool)
                .args(["-t", "get_bar_config", &bar_id])
                .output()
            else {
                continue;
            };
            let Ok(config) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
                continue;
            };
            if let Some(font) = config.get("font").and_then(|f| f.as_str()) {
                let status_command = config
                    .get("status_command")
                    .and_then(|c| c.as_str())
                    .unwrap_or_default()
                    .to_string();
                bars.push((bar_id, font.to_string(), status_command));
            }
        }
        if bars.is_empty() {
            continue;
        }
        let chosen = bars
            .iter()
            .position(|(_, _, cmd)| cmd.contains("i3status-rs"))
            .or_else(|| bars.iter().position(|(_, _, cmd)| cmd.contains("i3status")))
            .unwrap_or(0);
        let font = bars[chosen].1.clone();
        let note = if bars.iter().any(|(_, f, _)| *f != font) {
            let list = bars
                .iter()
                .map(|(id, f, _)| format!("{id}: {f:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            Some(format!(
                "several bars with different fonts ({list}); picked {:?} — pass --font if that \
                 is the wrong one",
                bars[chosen].0
            ))
        } else {
            None
        };
        return Some(DetectedFont {
            tool,
            bar_id: bars.swap_remove(chosen).0,
            font,
            note,
        });
    }
    None
}

/// Accept the i3/sway font directive as-is: strip the "pango:" prefix and
/// per-family trailing size and style options, split the fallback list on
/// commas.
///
/// `"pango:DejaVu Sans Mono Bold 12, Font Awesome 6 Free"` →
/// `["DejaVu Sans Mono", "Font Awesome 6 Free"]`
fn parse_font_directive(raw: &str) -> Vec<String> {
    let raw = raw.strip_prefix("pango:").unwrap_or(raw);
    raw.split(',')
        .map(strip_font_modifiers)
        .filter(|f| !f.is_empty())
        .collect()
}

/// Pango font descriptions are `FAMILY [STYLE-OPTIONS] [SIZE]`; drop the
/// trailing style words and size so only the family remains:
/// "DejaVu Sans Mono Bold 13.5" → "DejaVu Sans Mono"
fn strip_font_modifiers(family: &str) -> String {
    // Pango style, weight, stretch, variant and gravity keywords
    const STYLE_WORDS: &[&str] = &[
        "italic",
        "oblique",
        "roman",
        "thin",
        "ultralight",
        "ultra-light",
        "extralight",
        "extra-light",
        "light",
        "semilight",
        "semi-light",
        "demilight",
        "demi-light",
        "book",
        "regular",
        "normal",
        "medium",
        "semibold",
        "semi-bold",
        "demibold",
        "demi-bold",
        "bold",
        "ultrabold",
        "ultra-bold",
        "extrabold",
        "extra-bold",
        "heavy",
        "black",
        "ultraheavy",
        "ultra-heavy",
        "extrablack",
        "extra-black",
        "ultrablack",
        "ultra-black",
        "small-caps",
        "ultracondensed",
        "ultra-condensed",
        "extracondensed",
        "extra-condensed",
        "semicondensed",
        "semi-condensed",
        "condensed",
        "semiexpanded",
        "semi-expanded",
        "extraexpanded",
        "extra-expanded",
        "ultraexpanded",
        "ultra-expanded",
        "expanded",
        "not-rotated",
        "south",
        "upside-down",
        "north",
        "rotated-left",
        "east",
        "rotated-right",
        "west",
    ];
    let mut parts: Vec<&str> = family.split_whitespace().collect();
    while let Some(last) = parts.last() {
        let last = last.trim_end_matches("px");
        if last.parse::<f64>().is_ok() || STYLE_WORDS.contains(&last.to_lowercase().as_str()) {
            parts.pop();
        } else {
            break;
        }
    }
    parts.join(" ")
}

// ---------------------------------------------------------------------------
// Raw-config format string analysis
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract-based analysis of block `index` in `config`.
    fn contract(config: &str, index: usize) -> PlanAnalysis {
        let config: Config = toml::from_str(config).unwrap();
        let plan = config.blocks[index].config.plan().expect("prepare failed");
        analyze_plan(&plan)
    }

    #[test]
    fn contract_requires_only_reachable_icons() {
        // The design-doc example: the block can produce both phone and
        // phone_disconnected, but this configuration renders $icon only in
        // the connected state.
        let analysis = contract(
            r#"
            [[block]]
            block = "kdeconnect"
            format = " $icon "
            disconnected_format = " disconnected "
            missing_format = " missing "
            "#,
            0,
        );
        assert!(analysis.required.contains("phone"));
        assert!(!analysis.required.contains("phone_disconnected"));
    }

    #[test]
    fn contract_default_formats_require_state_icons() {
        // The default nordvpn driver has no connecting state, so
        // net_wireless is not a requirement.
        let analysis = contract("[[block]]\nblock = \"vpn\"", 0);
        for icon in ["net_vpn", "net_wired", "net_down"] {
            assert!(analysis.required.contains(icon), "{icon}");
        }
        assert!(!analysis.required.contains("net_wireless"));

        // Mullvad can report connecting; only the disconnected/error
        // formats lost their $icon, the other states still require theirs.
        let analysis = contract(
            "[[block]]\nblock = \"vpn\"\ndriver = \"mullvad\"\nformat_disconnected = \" off \"",
            0,
        );
        assert!(analysis.required.contains("net_vpn"));
        assert!(analysis.required.contains("net_wireless"));
        assert!(!analysis.required.contains("net_wired"));
        assert!(!analysis.required.contains("net_down"));
    }

    #[test]
    fn contract_inherited_formats_are_effective() {
        // charging_format inherits the configured `format`, which has no
        // $icon: bat_charging is not required. The state formats keep their
        // own defaults (" $icon "), so bat still is.
        let analysis = contract(
            "[[block]]\nblock = \"battery\"\nformat = \" $percentage \"",
            0,
        );
        assert!(!analysis.required.contains("bat_charging"));
        assert!(analysis.required.contains("bat"));
    }

    #[test]
    fn contract_configuration_derived_names() {
        let analysis = contract(
            r#"
            [[block]]
            block = "toggle"
            format = " $icon "
            command_on = ""
            command_off = ""
            command_state = ""
            icon_on = "my_enabled_icon"
            "#,
            0,
        );
        assert!(analysis.required.contains("my_enabled_icon"));
        assert!(analysis.required.contains("toggle_off"));
        assert!(!analysis.required.contains("toggle_on"));
    }

    #[test]
    fn contract_open_capability_is_reachability_gated() {
        // Only JSON output can carry an icon at all.
        let analysis = contract("[[block]]\nblock = \"custom\"\ncommand = \"true\"", 0);
        assert!(!analysis.open);

        let analysis = contract(
            "[[block]]\nblock = \"custom\"\ncommand = \"true\"\njson = true",
            0,
        );
        assert!(analysis.open);
        assert!(analysis.required.is_empty());

        // Neither the full nor the short format references $icon: the open
        // source is unreachable even with JSON.
        let analysis = contract(
            r#"
            [[block]]
            block = "custom"
            command = "true"
            json = true
            format = { full = " $text ", short = " $text " }
            "#,
            0,
        );
        assert!(!analysis.open);
    }

    #[test]
    fn contract_direct_icon_tokens_are_certain_usage() {
        let analysis = contract(
            "[[block]]\nblock = \"vpn\"\nformat_connected = \" ^icon_net_down $country \"",
            0,
        );
        assert!(analysis.direct.contains("net_down"));
        assert!(analysis.required.contains("net_down"));
        // Dead branch: "{ OK | ^icon_net_up }" never renders the icon.
        let analysis = contract(
            "[[block]]\nblock = \"vpn\"\nformat_connected = \"{ OK | ^icon_net_up }\"",
            0,
        );
        assert!(!analysis.required.contains("net_up"));
    }

    /// The error-widget analysis of block `index` in `config`.
    fn error_analysis(config: &str, index: usize) -> PlanAnalysis {
        let config: Config = toml::from_str(config).unwrap();
        let entry = &config.blocks[index];
        analyze_plan(&crate::block_plan::error_plan(
            entry
                .common
                .error_format
                .with_default_config(&config.error_format),
            entry
                .common
                .error_fullscreen_format
                .with_default_config(&config.error_fullscreen_format),
            entry.common.max_retries.is_some(),
        ))
    }

    #[test]
    fn empty_icon_names_are_runtime_noops() {
        // The runtime renders an empty icon name as empty output, so it is
        // never a requirement.
        let analysis = contract(
            r#"
            [[block]]
            block = "toggle"
            format = " $icon "
            command_on = ""
            command_off = ""
            command_state = ""
            icon_on = ""
            icon_off = ""
            "#,
            0,
        );
        assert!(analysis.required.is_empty());
    }

    #[test]
    fn refresh_requires_a_retry_limit() {
        // Without max_retries the block retries forever and the restart
        // button (and its refresh icon) can never appear.
        let analysis = error_analysis("[[block]]\nblock = \"cpu\"", 0);
        assert!(!analysis.required.contains("refresh"));

        let analysis = error_analysis("[[block]]\nblock = \"cpu\"\nmax_retries = 5", 0);
        assert!(analysis.required.contains("refresh"));
    }

    #[test]
    fn error_formats_without_restart_icon_do_not_require_refresh() {
        let analysis = error_analysis(
            r#"
            [[block]]
            block = "cpu"
            error_format = " $short_error_message "
            error_fullscreen_format = " $full_error_message "
            "#,
            0,
        );
        assert!(!analysis.required.contains("refresh"));
    }

    #[test]
    fn global_error_format_applies_to_blocks_without_overrides() {
        let analysis = error_analysis(
            "error_format = \" $short_error_message \"\nerror_fullscreen_format = \" e \"\n[[block]]\nblock = \"cpu\"",
            0,
        );
        assert!(!analysis.required.contains("refresh"));
    }

    #[test]
    fn font_directive_parsing() {
        assert_eq!(
            parse_font_directive("pango:DejaVu Sans Mono, Font Awesome 6 Free 12"),
            ["DejaVu Sans Mono", "Font Awesome 6 Free"]
        );
        assert_eq!(
            parse_font_directive("DejaVu Sans Mono 13.5"),
            ["DejaVu Sans Mono"]
        );
        // Pango style options are not part of the family name
        assert_eq!(
            parse_font_directive("pango:DejaVu Sans Mono Bold 10"),
            ["DejaVu Sans Mono"]
        );
        assert_eq!(
            parse_font_directive("Liberation Mono Bold Italic 11"),
            ["Liberation Mono"]
        );
        assert_eq!(
            parse_font_directive("Terminus (TTF) 16px"),
            ["Terminus (TTF)"]
        );
        assert_eq!(
            parse_font_directive("monospace,Font Awesome 5 Free"),
            ["monospace", "Font Awesome 5 Free"]
        );
        assert_eq!(parse_font_directive(""), Vec::<String>::new());
        assert_eq!(parse_font_directive("pango:"), Vec::<String>::new());
    }

    #[test]
    fn pango_families_from_icons_format() {
        assert_eq!(
            pango_font_families("<span font_family='Noto Color Emoji'>{icon}</span>"),
            ["Noto Color Emoji"]
        );
        assert_eq!(
            pango_font_families("<span face=\"Font Awesome 6 Free\" size='large'>{icon}</span>"),
            ["Font Awesome 6 Free"]
        );
        assert!(pango_font_families("{icon}").is_empty());
    }

    #[test]
    fn per_icon_live_relevance() {
        let live = LiveRelevance {
            rendered: ["memory_swap".to_string()].into_iter().collect(),
            provided_keys: [
                ("memory_swap".to_string(), "icon_swap".to_string()),
                ("memory_mem".to_string(), "icon_mem".to_string()),
            ]
            .into_iter()
            .collect(),
            rendered_keys: ["icon_swap".to_string()].into_iter().collect(),
        };
        assert!(live.is_relevant("memory_swap"));
        // provided under a placeholder the render never used
        assert!(!live.is_relevant("memory_mem"));
        // unknown pathway: live evidence is strictly positive; the static
        // analysis (scopes, per-format defaults) covers state siblings
        assert!(!live.is_relevant("bat_charging"));

        let none_rendered = LiveRelevance {
            rendered: HashSet::new(),
            provided_keys: [("time".to_string(), "icon".to_string())]
                .into_iter()
                .collect(),
            rendered_keys: HashSet::new(),
        };
        // { OK | $icon }: the icon branch never evaluated
        assert!(!none_rendered.is_relevant("time"));
        assert!(!none_rendered.is_relevant("anything_else"));
    }

    #[test]
    fn codepoint_formatting() {
        assert_eq!(codepoints("BAT"), "-");
        assert_eq!(codepoints("\u{f244}"), "U+F244");
        assert_eq!(codepoints("🍅"), "U+1F345");
    }

    #[test]
    fn generic_families_are_not_missing_fonts() {
        assert!(is_generic_family("monospace"));
        assert!(is_generic_family("Sans-Serif"));
        assert!(!is_generic_family("DejaVu Sans Mono"));
        assert!(!is_generic_family("Font Awesome 6 Free"));
    }

    #[test]
    fn per_block_overrides_are_extracted() {
        let raw: toml::Value = toml::from_str(
            r#"
            [[block]]
            block = "time"
            icons_overrides = { time = "CUSTOM" }

            [[block]]
            block = "cpu"
            "#,
        )
        .unwrap();
        let mut problems = Vec::new();
        let overrides = raw_block_overrides(&raw, &mut problems);
        assert!(problems.is_empty());
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].0, 0);
        assert!(matches!(&overrides[0].1["time"], Icon::Single(s) if s == "CUSTOM"));
    }

    #[test]
    fn instance_labels_disambiguate_duplicates() {
        let names = ["time".to_string(), "cpu".to_string(), "time".to_string()];
        assert_eq!(instance_labels(&names), ["time#1", "cpu", "time#3"]);
        let unique = ["time".to_string(), "cpu".to_string()];
        assert_eq!(instance_labels(&unique), ["time", "cpu"]);
    }

    #[test]
    fn fix_suggestions() {
        assert!(
            suggest_fix(
                "taskwarrior",
                "failed to run taskwarrior. Cause: No such file or directory"
            )
            .unwrap()
            .contains("Install")
        );
        assert!(
            suggest_fix("kdeconnect", "Failed to open DBus session connection")
                .unwrap()
                .contains("D-Bus")
        );
        assert!(suggest_fix("time", "something exotic").is_none());
    }
}
