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
//!   requested during the live test with each block's documented icon list
//!   (state-dependent icons a block did not request this cycle are marked
//!   "may").
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

use futures::StreamExt as _;
use tokio::sync::mpsc;

use crate::blocks::CommonApi;
use crate::config::{BlockConfigEntry, Config, SharedConfig};
use crate::errors::*;
use crate::formatting::parse as format_parse;
use crate::formatting::value::ValueInner;
use crate::geolocator::Geolocator;
use crate::icons::{Icon, Icons};
use crate::util;
use crate::widget::Widget;
use crate::{Request, RequestCmd};

include!(concat!(env!("OUT_DIR"), "/block_icons.rs"));
include!(concat!(env!("OUT_DIR"), "/block_icon_keys.rs"));
include!(concat!(env!("OUT_DIR"), "/block_format_defaults.rs"));
include!(concat!(env!("OUT_DIR"), "/block_icon_format_scopes.rs"));
include!(concat!(env!("OUT_DIR"), "/block_icon_configs.rs"));

/// The format keys an icon is scoped to (state-specific formats). Empty =
/// the icon can be rendered by any of the block's formats.
fn icon_format_scopes(block_type: &str, icon: &str) -> Vec<&'static str> {
    match BLOCK_ICON_FORMAT_SCOPES.binary_search_by_key(&block_type, |(block, _)| block) {
        Ok(i) => BLOCK_ICON_FORMAT_SCOPES[i]
            .1
            .iter()
            .filter(|(name, _)| *name == icon)
            .map(|(_, key)| *key)
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// (format key, default kind, default value) rows for a block type.
fn format_defaults_for(block_type: &str) -> &'static [(&'static str, &'static str, &'static str)] {
    match BLOCK_FORMAT_DEFAULTS.binary_search_by_key(&block_type, |(block, _)| block) {
        Ok(i) => BLOCK_FORMAT_DEFAULTS[i].1,
        Err(_) => &[],
    }
}

/// Config keys that rename a canonical icon for a block type.
fn icon_config_renames(block_type: &str) -> &'static [(&'static str, &'static str)] {
    match BLOCK_ICON_CONFIGS.binary_search_by_key(&block_type, |(block, _)| block) {
        Ok(i) => BLOCK_ICON_CONFIGS[i].1,
        Err(_) => &[],
    }
}

/// The placeholder keys a block type provides the given icon name under,
/// according to the statically scanned `"key" => Value::icon("name")` pairs.
fn icon_keys_for(block_type: &str, icon: &str) -> Vec<&'static str> {
    match BLOCK_ICON_KEYS.binary_search_by_key(&block_type, |(block, _)| block) {
        Ok(i) => BLOCK_ICON_KEYS[i]
            .1
            .iter()
            .filter(|(_, name)| *name == icon)
            .map(|(key, _)| *key)
            .collect(),
        Err(_) => Vec::new(),
    }
}

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
    // Config keys that rename a canonical icon (e.g. toggle's icon_on):
    // label -> [(custom name, canonical name)]
    let mut renames: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let raw_blocks = raw.get("block").and_then(|v| v.as_array());
    // ^icon_* references in format strings count as explicit usage
    for (index, info) in collect_blocks(&raw).iter().enumerate() {
        let table = raw_blocks.and_then(|blocks| blocks.get(index));
        icon_relevant.insert(
            labels[index].clone(),
            IconRelevance {
                static_rel: StaticRelevance::compute(&block_names[index], table),
                live: None,
            },
        );
        for (config_key, canonical) in icon_config_renames(&block_names[index]) {
            if let Some(custom) = table
                .and_then(|t| t.get(*config_key))
                .and_then(|v| v.as_str())
            {
                renames
                    .entry(labels[index].clone())
                    .or_default()
                    .push((custom.to_string(), (*canonical).to_string()));
            }
        }
        for format in &info.formats {
            for icon in &format.icon_refs {
                used_now
                    .entry(icon.clone())
                    .or_default()
                    .insert(labels[index].clone());
            }
        }
    }

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
                        ..
                    } => {
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
                        // (if_command failed): nothing it could request
                        if let Some(relevance) = icon_relevant.get_mut(&label) {
                            relevance.static_rel = StaticRelevance {
                                sets: HashMap::new(),
                            };
                            relevance.live = None;
                        }
                    }
                    LiveVerdict::RenderError { error, .. } => {
                        if let Some(icon) = error
                            .split("Icon '")
                            .nth(1)
                            .and_then(|rest| rest.split('\'').next())
                        {
                            live_reported.insert((label.clone(), icon.to_string()));
                        }
                        // rendering failed, possibly on an icon: everything
                        // stays relevant
                        if let Some(relevance) = icon_relevant.get_mut(&label) {
                            let mut sets = HashMap::new();
                            sets.insert(
                                "format".to_string(),
                                FormatSet {
                                    wildcard: true,
                                    ..Default::default()
                                },
                            );
                            relevance.static_rel = StaticRelevance { sets };
                        }
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
        block_names: &block_names,
        used_now: &used_now,
        icon_relevant: &icon_relevant,
        renames: &renames,
        live_reported: &live_reported,
        live_ran,
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
    /// The if_command could not run or did not finish — unlike a legitimate
    /// non-zero exit (`Skipped`), this is a problem.
    IfCommandFailed(String),
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
            diagnosis: "Cannot register as a child subreaper; processes spawned by blocks that                         detach from their process group may outlive doctor."
                .into(),
            fix: None,
        });
    }

    let reports = runtime.block_on(async {
        let workers = (0..count).map(|index| {
            let exe = exe.clone();
            async move { run_worker_process(&exe, config_arg, index).await }
        });
        futures::future::join_all(workers).await
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
        Err(_) => fail(format!(
            "doctor worker did not finish within {}s and was killed",
            (LIVE_TIMEOUT + WORKER_GRACE).as_secs()
        )),
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
                    verdict: LiveVerdict::IfCommandFailed(format!(
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
            problems.push(Problem {
                diagnosis: format!("{}: {error}", report.name),
                fix: suggest_fix(&report.name, error),
            });
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
                diagnosis: format!("{}: {reason}", report.name),
                fix: Some(
                    "if_command must finish quickly and exit 0 or non-zero; make it fast and \
                     non-blocking, or remove it."
                        .into(),
                ),
            });
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

/// The effective reachable-placeholder set of every primary format of a
/// block instance: configured formats are parsed; not-configured (or partially
/// configured) ones use their per-key defaults from the generated table,
/// following `inherits` chains. Unknown defaults degrade to a wildcard.
/// Reachable content of one format: placeholder names, direct ^icon_* names,
/// and whether the format is unknown (wildcard).
#[derive(Default)]
struct FormatSet {
    placeholders: HashSet<String>,
    icons: HashSet<String>,
    wildcard: bool,
}

struct StaticRelevance {
    /// format key -> reachable content
    sets: HashMap<String, FormatSet>,
}

impl StaticRelevance {
    fn compute(block_type: &str, table: Option<&toml::Value>) -> Self {
        let defaults = format_defaults_for(block_type);
        let mut keys: HashSet<String> = defaults.iter().map(|(k, ..)| (*k).to_string()).collect();
        if let Some(table) = table.and_then(|t| t.as_table()) {
            for key in table.keys() {
                if matches!(format_key_kind(key), Some(false)) {
                    keys.insert(key.clone());
                }
            }
        }
        if keys.is_empty() {
            // no metadata at all: stay conservative
            let mut sets = HashMap::new();
            sets.insert(
                "format".to_string(),
                FormatSet {
                    wildcard: true,
                    ..Default::default()
                },
            );
            return Self { sets };
        }
        let mut sets = HashMap::new();
        for key in &keys {
            sets.insert(key.clone(), Self::effective(block_type, key, table, 0));
        }
        Self { sets }
    }

    /// The reachable placeholders of format `key`, considering the
    /// configured value and the per-key default (with inheritance).
    fn effective(
        block_type: &str,
        key: &str,
        table: Option<&toml::Value>,
        depth: usize,
    ) -> FormatSet {
        if depth > 4 {
            return FormatSet {
                wildcard: true,
                ..Default::default()
            };
        }
        let configured = table.and_then(|t| t.as_table()).and_then(|t| t.get(key));
        match configured {
            Some(toml::Value::String(s)) => template_set(s),
            Some(toml::Value::Table(parts)) => {
                let mut set = FormatSet::default();
                if let Some(toml::Value::String(s)) = parts.get("short") {
                    set.merge(template_set(s));
                }
                match parts.get("full") {
                    Some(toml::Value::String(s)) => set.merge(template_set(s)),
                    // partial table: the full part comes from this key's
                    // OWN default, not the block-wide default
                    _ => set.merge(Self::default_of(block_type, key, table, depth)),
                }
                set
            }
            _ => Self::default_of(block_type, key, table, depth),
        }
    }

    fn default_of(
        block_type: &str,
        key: &str,
        table: Option<&toml::Value>,
        depth: usize,
    ) -> FormatSet {
        match format_defaults_for(block_type)
            .iter()
            .find(|(k, ..)| *k == key)
        {
            Some((_, "literal", value)) => template_set(value),
            Some((_, "inherits", target)) => Self::effective(block_type, target, table, depth + 1),
            // an Option format that is not configured has no format at all
            Some((_, kind, _)) if kind.starts_with("optional") => FormatSet::default(),
            _ => FormatSet {
                wildcard: true,
                ..Default::default()
            },
        }
    }

    fn is_relevant(&self, icon: &str, block_type: &str) -> bool {
        // The generated key table is complete (enforced at build time), so
        // an icon is reachable exactly when one of its placeholders is
        // referenced by a format it is scoped to, or a format names it as a
        // direct ^icon token (e.g. default formats of speedtest and net).
        // Icons with no known key (e.g. dynamic custom-block names) have no
        // static pathway; the live test and used_now cover them.
        let icon_keys = icon_keys_for(block_type, icon);
        let scopes = icon_format_scopes(block_type, icon);
        self.sets
            .iter()
            .filter(|(key, _)| scopes.is_empty() || scopes.contains(&key.as_str()))
            .any(|(_, set)| {
                (set.wildcard && !icon_keys.is_empty())
                    || set.icons.contains(icon)
                    || icon_keys.iter().any(|k| set.placeholders.contains(*k))
            })
    }
}

impl FormatSet {
    fn merge(&mut self, other: FormatSet) {
        self.placeholders.extend(other.placeholders);
        self.icons.extend(other.icons);
        self.wildcard |= other.wildcard;
    }
}

/// The reachable placeholders and direct icons of one template string.
fn template_set(s: &str) -> FormatSet {
    let mut placeholders = Vec::new();
    let mut icons = Vec::new();
    if let Ok(template) = format_parse::parse_full(s) {
        collect_reachable(&template, &mut placeholders, &mut icons);
    }
    FormatSet {
        placeholders: placeholders.into_iter().collect(),
        icons: icons.into_iter().collect(),
        wildcard: false,
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
    static_rel: StaticRelevance,
    live: Option<LiveRelevance>,
}

impl IconRelevance {
    fn is_relevant(&self, icon: &str, block_type: &str) -> bool {
        self.static_rel.is_relevant(icon, block_type)
            || self.live.as_ref().is_some_and(|l| l.is_relevant(icon))
    }
}

struct IconTableInput<'a> {
    base_map: &'a HashMap<String, Icon>,
    global_overrides: &'a HashMap<String, Icon>,
    block_overrides: &'a [(String, HashMap<String, Icon>)],
    builtin: &'a HashMap<String, Icon>,
    block_names: &'a [String],
    used_now: &'a BTreeMap<String, HashSet<String>>,
    /// Per block label: what is known about its ability to render each icon.
    icon_relevant: &'a HashMap<String, IconRelevance>,
    /// Per block label: config-driven icon renames [(custom, canonical)].
    renames: &'a HashMap<String, Vec<(String, String)>>,
    /// (label, icon) pairs already diagnosed by a live render error.
    live_reported: &'a HashSet<(String, String)>,
    /// Whether the live block test ran (skipping it makes some checks
    /// inconclusive).
    live_ran: bool,
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
        block_names,
        used_now,
        icon_relevant,
        renames,
        live_reported,
        live_ran,
        font_check,
        font_authoritative,
        style,
        problems,
    } = input;
    // Per-block-type local overrides (several blocks of the same type are
    // merged, later wins — matching how "used by" is attributed by type).
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

    // may-use: documented icons of each configured block type, attributed to
    // each instance's label, with config-driven renames applied (toggle's
    // icon_on="custom" means the instance may request "custom", and the
    // canonical name is NOT requested)
    let labels = instance_labels(block_names);
    let label_types: HashMap<String, String> = labels
        .iter()
        .cloned()
        .zip(block_names.iter().cloned())
        .collect();
    let mut may_use: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (index, name) in block_names.iter().enumerate() {
        if let Ok(i) = BLOCK_ICONS.binary_search_by_key(&name.as_str(), |(block, _)| block) {
            for icon in BLOCK_ICONS[i].1 {
                let effective = renames
                    .get(&labels[index])
                    .and_then(|r| r.iter().find(|(_, canonical)| canonical == icon))
                    .map(|(custom, _)| custom.clone())
                    .unwrap_or_else(|| (*icon).to_string());
                may_use
                    .entry(effective)
                    .or_default()
                    .push(labels[index].clone());
            }
        }
    }

    // Blocks of the custom family can request arbitrary icon names at
    // runtime, so an override nothing references statically may still be
    // used dynamically.
    let dynamic_blocks = block_names
        .iter()
        .any(|n| n == "custom" || n == "custom_dbus");
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
            if live_ran && !dynamic_blocks {
                problems.push(Problem {
                    diagnosis,
                    fix: None,
                });
            } else {
                // Inconclusive: dynamic (custom) blocks may request it, and
                // without the live test that cannot be observed.
                println!(
                    "note: {diagnosis} Not counted as a problem: dynamic icon usage cannot be ruled out{}.",
                    if live_ran {
                        ""
                    } else {
                        " without the live test"
                    }
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
    for (icon, blocks) in &may_use {
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
                users.push((block.clone(), false));
            }
        }
    }

    let mut used_rows: Vec<IconRow> = Vec::new();
    let mut fallback_rows = 0usize;
    let mut missing_rows = 0usize;

    for (icon_name, users) in &usage {
        // Group the using blocks by what the icon actually resolves to for
        // them: an override REPLACES the base glyph for its block, so only
        // glyphs some block really renders produce rows.
        let mut groups: BTreeMap<Provenance, Vec<String>> = BTreeMap::new();
        for (block, is_may) in users {
            // A block whose formats cannot render this icon cannot error on
            // it; skip latent (may) findings for it. Renamed icons (toggle's
            // icon_on="custom") are canonicalized for the key/scope lookups.
            let block_type = label_types.get(block).map(String::as_str).unwrap_or(block);
            let canonical_name = renames
                .get(block)
                .and_then(|r| r.iter().find(|(custom, _)| custom == icon_name))
                .map(|(_, canonical)| canonical.as_str())
                .unwrap_or(icon_name);
            let relevant = icon_relevant
                .get(block)
                .map(|r| r.is_relevant(canonical_name, block_type))
                .unwrap_or(true);
            match resolve(icon_name, block) {
                Some((icon, provenance)) => {
                    // An empty progression is stored but Icons::get returns
                    // None for it: at runtime it behaves like an undefined
                    // icon, not like a defined one.
                    if matches!(icon, Icon::Progression(steps) if steps.is_empty()) {
                        if !*is_may || relevant {
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
                None if *is_may && !relevant => (),
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

    // Defined but unused by every configured block
    let mut unused: Vec<&str> = Vec::new();
    for name in base_map.keys().chain(global_overrides.keys()) {
        if !usage.contains_key(name) && !unused.contains(&name.as_str()) {
            unused.push(name);
        }
    }
    unused.sort_unstable();

    println!("Icons referenced by your blocks");
    let name_w = column_width("name", used_rows.iter().map(|r| r.name.as_str()));
    let codes_w = column_width("code", used_rows.iter().map(|r| r.codes.as_str()));
    let provider_w = column_width("provider", used_rows.iter().map(|r| r.provider.as_str()));
    println!(
        "{:<name_w$}  {:<5} {:<codes_w$}  {:<provider_w$}  used by",
        "name", "glyph", "code", "provider"
    );
    for row in &used_rows {
        let line = format!(
            "{:<name_w$}  \"{}\"   {:<codes_w$}  {:<provider_w$}  {}",
            row.name, row.glyph, row.codes, row.provider, row.used_by
        );
        if row.red {
            println!("{}{line}{}", style.red, style.reset);
        } else {
            println!("{line}");
        }
    }
    if fallback_rows > 0 {
        println!("* Glyph provider selected by the system because no font in your list has it.");
    }
    if missing_rows > 0 {
        println!("† No installed font has this glyph; it renders as an empty box.");
    }
    if !unused.is_empty() {
        println!(
            "Defined but not used by any configured block ({}): {}",
            unused.len(),
            unused.join(", ")
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
                 state-dependent and runtime behavior was not checked)"
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

struct FormatUse {
    /// ^icon_* references in reachable branches.
    icon_refs: Vec<String>,
}

struct BlockInfo {
    formats: Vec<FormatUse>,
}

fn collect_blocks(raw: &toml::Value) -> Vec<BlockInfo> {
    let Some(blocks) = raw.get("block").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    blocks
        .iter()
        .map(|block| {
            let mut formats = Vec::new();
            collect_formats(block, &mut formats);
            BlockInfo { formats }
        })
        .collect()
}

/// Whether a key holds a format template. `icons_format` is not a template,
/// and error formats are classified separately: they only render on errors.
fn format_key_kind(key: &str) -> Option<bool /* is_error_format */> {
    if key == "icons_format" {
        return None;
    }
    if key == "error_format" || key == "error_fullscreen_format" {
        return Some(true);
    }
    // "format", suffixed variants like "format_alt" / "format_singular", and
    // prefixed variants like "inactive_format" / "full_format"
    (key == "format" || key.starts_with("format_") || key.ends_with("_format")).then_some(false)
}

/// Recursively find format templates under `format` / `*_format` keys and
/// extract what they reference. A format value can be a plain string or the
/// table form `{ full = "...", short = "..." }`.
fn collect_formats(value: &toml::Value, out: &mut Vec<FormatUse>) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (key, value) in table {
        match (format_key_kind(key), value) {
            (Some(_), toml::Value::String(s)) => push_format_use(s, out),
            (Some(_), toml::Value::Table(parts)) => {
                // { full = "...", short = "..." } form
                for part in ["full", "short"] {
                    if let Some(toml::Value::String(s)) = parts.get(part) {
                        push_format_use(s, out);
                    }
                }
            }
            (_, toml::Value::Table(_)) => collect_formats(value, out),
            (_, toml::Value::Array(array)) => {
                for item in array {
                    collect_formats(item, out);
                }
            }
            _ => (),
        }
    }
}

fn push_format_use(s: &str, out: &mut Vec<FormatUse>) {
    if let Ok(template) = format_parse::parse_full(s) {
        let mut icon_refs = Vec::new();
        let mut placeholders = Vec::new();
        collect_reachable(&template, &mut placeholders, &mut icon_refs);
        out.push(FormatUse { icon_refs });
    }
}

/// Walk only the *reachable* branches of a template: alternatives after a
/// branch that cannot fail (only literal text) are dead — "{ OK | $icon }"
/// never evaluates $icon.
fn collect_reachable(
    template: &format_parse::FormatTemplate,
    placeholders: &mut Vec<String>,
    icon_refs: &mut Vec<String>,
) {
    for token_list in &template.0 {
        let mut branch_can_fail = false;
        for token in &token_list.0 {
            match token {
                format_parse::Token::Placeholder(placeholder) => {
                    placeholders.push(placeholder.name.to_string());
                    branch_can_fail = true;
                }
                format_parse::Token::Icon(name) => {
                    // A missing icon is a render error, not a branch-selection
                    // failure: it does not make the branch fall through.
                    icon_refs.push((*name).to_string());
                }
                format_parse::Token::Recursive(rec) => {
                    collect_reachable(rec, placeholders, icon_refs);
                    branch_can_fail |= group_can_fail(rec);
                }
                format_parse::Token::Text(_) => (),
            }
        }
        if !branch_can_fail {
            break;
        }
    }
}

/// A group fails only if every one of its branches can fail.
fn group_can_fail(template: &format_parse::FormatTemplate) -> bool {
    template.0.iter().all(|token_list| {
        token_list.0.iter().any(|token| match token {
            format_parse::Token::Placeholder(_) => true,
            format_parse::Token::Recursive(rec) => group_can_fail(rec),
            // A missing icon errors the whole render instead of selecting
            // the next branch.
            format_parse::Token::Icon(_) | format_parse::Token::Text(_) => false,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relevance(config: &str, index: usize, block_type: &str) -> StaticRelevance {
        let raw: toml::Value = toml::from_str(config).unwrap();
        let table = raw
            .get("block")
            .and_then(|b| b.as_array())
            .and_then(|b| b.get(index));
        StaticRelevance::compute(block_type, table)
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
    fn block_icons_table_is_generated() {
        assert!(!BLOCK_ICONS.is_empty());
        let battery = BLOCK_ICONS
            .iter()
            .find(|(block, _)| *block == "battery")
            .expect("battery block missing from generated table");
        assert!(battery.1.contains(&"bat_charging"));
        // the table must be sorted for binary_search_by_key
        assert!(BLOCK_ICONS.windows(2).all(|w| w[0].0 <= w[1].0));

        // completeness regressions: icons the docs historically missed, now
        // covered by the doc lists plus the source literal scan
        let icons_of = |block: &str| {
            BLOCK_ICONS
                .iter()
                .find(|(b, _)| *b == block)
                .unwrap_or_else(|| panic!("{block} missing from BLOCK_ICONS"))
                .1
        };
        assert!(icons_of("music").contains(&"music_pause"));
        assert!(icons_of("weather").contains(&"weather_default"));
        assert!(icons_of("bluetooth").contains(&"bat"));
        // heading variants: "#  Icons Used" and "# Used Icons"
        assert!(icons_of("sound").contains(&"volume"));
        assert!(icons_of("uptime").contains(&"uptime"));
        assert!(icons_of("xrandr").contains(&"xrandr"));
    }

    #[test]
    fn format_icon_ref_extraction() {
        let raw: toml::Value = toml::from_str(
            r#"
            [[block]]
            block = "battery"
            format = " ^icon_bat $percentage "
            full_format = " {^icon_bat_charging |}rest "
            icons_format = "not_a_template"
            [block.nested]
            some_format = "^icon_nested_ref"
            "#,
        )
        .unwrap();
        let blocks = collect_blocks(&raw);
        let refs: Vec<&str> = blocks[0]
            .formats
            .iter()
            .flat_map(|f| f.icon_refs.iter().map(String::as_str))
            .collect();
        assert!(refs.contains(&"bat"));
        assert!(refs.contains(&"bat_charging"));
        assert!(refs.contains(&"nested_ref"));
        assert!(!refs.iter().any(|r| r.contains("not_a_template")));
    }

    #[test]
    fn format_key_classification() {
        assert_eq!(format_key_kind("format"), Some(false));
        assert_eq!(format_key_kind("format_alt"), Some(false));
        assert_eq!(format_key_kind("format_singular"), Some(false));
        assert_eq!(format_key_kind("full_format"), Some(false));
        assert_eq!(format_key_kind("inactive_format"), Some(false));
        assert_eq!(format_key_kind("error_format"), Some(true));
        assert_eq!(format_key_kind("error_fullscreen_format"), Some(true));
        assert_eq!(format_key_kind("icons_format"), None);
        assert_eq!(format_key_kind("command"), None);
    }

    #[test]
    fn table_form_formats_and_error_formats() {
        let config = r#"
            [[block]]
            block = "time"
            error_format = "$full_error_message"
            [block.format]
            full = " $timestamp "
            short = " t "

            [[block]]
            block = "time"
            error_format = "$full_error_message"
            "#;
        // table-form format with `full` is an explicit icon-free primary
        // format: time (under $icon) is unreachable
        assert!(!relevance(config, 0, "time").is_relevant("time", "time"));
        // error_format alone is not evidence of a primary format: the
        // default time format (which renders $icon) still applies
        assert!(relevance(config, 1, "time").is_relevant("time", "time"));
    }

    #[test]
    fn partial_table_format_inherits_default() {
        // no `full` member: the full part comes from the KEY's own default —
        // time's default format renders $icon, memory's format_alt default
        // is empty
        let config = r#"
            [[block]]
            block = "time"
            [block.format]
            short = " SHORT "

            [[block]]
            block = "memory"
            format = " MEM "
            [block.format_alt]
            short = " S "
            "#;
        assert!(relevance(config, 0, "time").is_relevant("time", "time"));
        let memory = relevance(config, 1, "memory");
        assert!(!memory.is_relevant("memory_mem", "memory"));
        assert!(!memory.is_relevant("memory_swap", "memory"));
    }

    #[test]
    fn default_format_icon_tokens_are_required() {
        // speedtest's default format renders ^icon_ping etc. directly
        let config = r#"
            [[block]]
            block = "speedtest"
            "#;
        let speedtest = relevance(config, 0, "speedtest");
        assert!(speedtest.is_relevant("ping", "speedtest"));
        assert!(speedtest.is_relevant("net_down", "speedtest"));
        // an explicit format without the tokens drops them
        let config = r#"
            [[block]]
            block = "speedtest"
            format = " $ping "
            "#;
        assert!(!relevance(config, 0, "speedtest").is_relevant("net_down", "speedtest"));
    }

    #[test]
    fn state_scopes_pomodoro_and_vpn() {
        let config = r#"
            [[block]]
            block = "pomodoro"
            format = "$icon"
            pomodoro_format = "$status_icon"
            break_format = " brk "

            [[block]]
            block = "vpn"
            format_connected = " $icon "
            format_disconnected = " off "
            format_connecting = " ... "
            "#;
        let pomodoro = relevance(config, 0, "pomodoro");
        assert!(pomodoro.is_relevant("pomodoro_started", "pomodoro"));
        assert!(!pomodoro.is_relevant("pomodoro_break", "pomodoro"));
        assert!(!pomodoro.is_relevant("pomodoro_stopped", "pomodoro"));
        let vpn = relevance(config, 1, "vpn");
        assert!(vpn.is_relevant("net_vpn", "vpn"));
        assert!(!vpn.is_relevant("net_wired", "vpn"));
        assert!(!vpn.is_relevant("net_wireless", "vpn"));
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
    fn state_scoped_formats() {
        // battery's bat_charging is scoped to charging_format and
        // bat_not_available to missing_format: making those literal-only
        // removes the requirement while `format` keeps bat required
        let config = r#"
            [[block]]
            block = "battery"
            format = " $icon $percentage "
            charging_format = " chg "
            missing_format = " none "
            "#;
        let battery = relevance(config, 0, "battery");
        assert!(battery.is_relevant("bat", "battery"));
        assert!(!battery.is_relevant("bat_charging", "battery"));
        assert!(!battery.is_relevant("bat_not_available", "battery"));

        // charging_format inherits the configured `format` when unset
        let config = r#"
            [[block]]
            block = "battery"
            format = " $icon $percentage "
            "#;
        assert!(relevance(config, 0, "battery").is_relevant("bat_charging", "battery"));

        let config = r#"
            [[block]]
            block = "battery"
            format = " $percentage "
            "#;
        assert!(!relevance(config, 0, "battery").is_relevant("bat_charging", "battery"));
    }

    #[test]
    fn toggle_icon_renames_are_scanned() {
        assert!(
            icon_config_renames("toggle")
                .iter()
                .any(|(key, name)| *key == "icon_on" && *name == "toggle_on")
        );
    }

    #[test]
    fn branch_reachability() {
        // the literal first branch always succeeds: $icon is dead
        let template = format_parse::parse_full("{ OK | $icon }").unwrap();
        let mut placeholders = Vec::new();
        let mut icons = Vec::new();
        collect_reachable(&template, &mut placeholders, &mut icons);
        assert!(placeholders.is_empty());

        // a placeholder branch can fail: the fallback stays reachable
        let template = format_parse::parse_full("{ $a | $icon }").unwrap();
        let mut placeholders = Vec::new();
        let mut icons = Vec::new();
        collect_reachable(&template, &mut placeholders, &mut icons);
        assert_eq!(placeholders, ["a", "icon"]);

        // a missing ^icon is a render error, not a branch fall-through:
        // the second branch is dead
        let template = format_parse::parse_full("{ ^icon_time | ^icon_memory_mem }").unwrap();
        let mut placeholders = Vec::new();
        let mut icons = Vec::new();
        collect_reachable(&template, &mut placeholders, &mut icons);
        assert_eq!(icons, ["time"]);

        // but after a fallible placeholder branch, an icon branch is reachable
        let template = format_parse::parse_full("{ $a | ^icon_time }").unwrap();
        let mut placeholders = Vec::new();
        let mut icons = Vec::new();
        collect_reachable(&template, &mut placeholders, &mut icons);
        assert_eq!(icons, ["time"]);
    }

    #[test]
    fn helper_created_icon_keys_are_mapped() {
        // music's `"next" => new_btn("music_next", ...)` goes through a
        // helper; the generated table must still map it
        assert!(icon_keys_for("music", "music_next").contains(&"next"));
        assert!(icon_keys_for("music", "music_prev").contains(&"prev"));
        // direct form still works
        assert!(icon_keys_for("memory", "memory_swap").contains(&"icon_swap"));
    }

    #[test]
    fn icon_refs_do_not_make_unmapped_icons_relevant() {
        let config = r#"
            [[block]]
            block = "speedtest"
            format = " ^icon_ping $ping "
            "#;
        // ^icon_ping names one specific icon; net_up/net_down (token icons
        // with no placeholder) must not become relevant because of it
        let speedtest = relevance(config, 0, "speedtest");
        assert!(!speedtest.is_relevant("net_up", "speedtest"));
        assert!(!speedtest.is_relevant("net_down", "speedtest"));
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
    fn static_icon_relevance() {
        let config = r#"
            [[block]]
            block = "time"
            format = " $timestamp "

            [[block]]
            block = "time"
            format = " $icon $timestamp "

            [[block]]
            block = "battery"

            [[block]]
            block = "net"
            format = " ^icon_net_wireless $ssid "

            [[block]]
            block = "memory"
            format = " $icon_swap $mem_used_percents "
            "#;
        // explicit format without icons: icons are irrelevant
        assert!(!relevance(config, 0, "time").is_relevant("time", "time"));
        // explicit format with $icon placeholder: time travels under $icon
        assert!(relevance(config, 1, "time").is_relevant("time", "time"));
        // no explicit format: the defaults render icons
        assert!(relevance(config, 2, "battery").is_relevant("bat", "battery"));
        // a ^icon_* reference names one specific icon: it does not make
        // other icons relevant (they are checked via used_now instead)
        assert!(!relevance(config, 3, "net").is_relevant("net_up", "net"));
        // per-icon: $icon_swap requires memory_swap but not memory_mem
        let memory = relevance(config, 4, "memory");
        assert!(memory.is_relevant("memory_swap", "memory"));
        assert!(!memory.is_relevant("memory_mem", "memory"));
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
