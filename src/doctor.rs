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
        print_problems(&problems, &style);
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
        print_problems(&problems, &style);
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
    let mut font_check = FontCheck::new(font_pattern);
    // Environment limitations are notes, not problems: they say what doctor
    // could not check, not that the user's configuration is wrong, and must
    // not affect the exit code.
    match (&font_check, font_arg, &detected) {
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
                 providers below may not match your actual bar. Re-run with the `font`\n   \
                 directive from the bar {{ }} section of your i3/sway config:\n   \
                 i3status-rs --doctor --font \"pango:...\""
            );
        }
    }
    if let Some(check) = &font_check {
        for (family, resolved) in &check.families {
            // Generic fontconfig aliases (monospace, sans-serif, ...) always
            // resolve to some concrete family; that is normal, not a missing
            // font.
            if is_generic_family(family) {
                continue;
            }
            let installed = resolved
                .as_ref()
                .is_some_and(|r| r.split(',').any(|m| m == family));
            if !installed {
                problems.push(Problem {
                    diagnosis: format!(
                        "Font {family:?} is in the bar's font list but not installed{}.",
                        resolved
                            .as_ref()
                            .map(|r| format!(" (fontconfig silently uses {r:?} in its place)"))
                            .unwrap_or_default()
                    ),
                    fix: Some(format!(
                        "Install {family:?}, or remove it from the bar's font directive."
                    )),
                });
            }
        }
    }
    println!();

    // === Blocks (live test) ===
    let block_names = raw_block_names(&raw);
    let mut used_now: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    // ^icon_* references in format strings count as explicit usage
    for info in collect_blocks(&raw) {
        for format in &info.formats {
            for icon in &format.icon_refs {
                used_now
                    .entry(icon.clone())
                    .or_default()
                    .insert(info.name.clone());
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

    match (parsed, skip_live) {
        (None, _) => println!("Blocks: skipped (configuration does not validate)\n"),
        (Some(_), true) => println!("Blocks: skipped (--doctor-skip-live)\n"),
        (Some(mut config), false) => {
            println!(
                "Blocks (each run for one cycle; performs real requests/commands, {}s timeout)",
                LIVE_TIMEOUT.as_secs()
            );
            let blocks = std::mem::take(&mut config.blocks);
            let reports = run_live(blocks, &config);
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
                print_block_report(report, tag_w, out_w, &style, &mut problems, &mut used_now);
            }
            println!();
        }
    }

    // === Icons table ===
    let block_overrides = raw_block_overrides(&raw, &mut problems);
    print_icon_table(
        &base_map,
        &global_overrides,
        &block_overrides,
        &builtin,
        &block_names,
        &used_now,
        &mut font_check,
        &style,
        &mut problems,
    );

    print_problems(&problems, &style);
    problems.len()
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

/// Per-block `icons_overrides` tables from the raw config, with block names.
fn raw_block_overrides(
    raw: &toml::Value,
    problems: &mut Vec<Problem>,
) -> Vec<(String, HashMap<String, Icon>)> {
    let Some(blocks) = raw.get("block").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for block in blocks {
        let name = block
            .get("block")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let Some(value) = block.get("icons_overrides") else {
            continue;
        };
        match value.clone().try_into() {
            Ok(overrides) => out.push((name.to_string(), overrides)),
            Err(err) => problems.push(Problem {
                diagnosis: format!("{name}: icons_overrides is not a valid icon table: {err}"),
                fix: None,
            }),
        }
    }
    out
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

enum LiveVerdict {
    Rendered {
        text: String,
        icons: Vec<String>,
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
}

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

fn run_live(blocks: Vec<BlockConfigEntry>, config: &Config) -> Vec<BlockReport> {
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

    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, async {
        let mut handles = Vec::new();
        for (index, entry) in blocks.into_iter().enumerate() {
            let shared = config.shared.clone();
            let geolocator = config.geolocator.clone();
            handles.push(tokio::task::spawn_local(test_block(
                index, entry, shared, geolocator,
            )));
        }
        let mut reports = Vec::new();
        for (index, handle) in handles.into_iter().enumerate() {
            reports.push(match handle.await {
                Ok(report) => report,
                Err(err) => BlockReport {
                    index,
                    name: "<unknown>".into(),
                    verdict: LiveVerdict::Panicked(join_error_message(err)),
                },
            });
        }
        reports
    })
}

async fn test_block(
    index: usize,
    entry: BlockConfigEntry,
    mut shared: SharedConfig,
    geolocator: Arc<Geolocator>,
) -> BlockReport {
    let name = entry.config.name().to_string();

    if let Some(cmd) = &entry.common.if_command {
        // The command counts against the block's timeout. It runs in its own
        // process group so that on timeout the whole tree can be killed — a
        // hanging if_command must neither hang doctor nor leak processes.
        let mut command = tokio::process::Command::new("sh");
        command
            .args(["-c", cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                return BlockReport {
                    index,
                    name,
                    verdict: LiveVerdict::Skipped(format!("if_command could not run: {err}")),
                };
            }
        };
        match tokio::time::timeout(LIVE_TIMEOUT, child.wait()).await {
            Err(_) => {
                if let Some(pid) = child.id() {
                    // SAFETY: plain syscall; negative pid targets the group
                    unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
                }
                let _ = child.wait().await;
                return BlockReport {
                    index,
                    name,
                    verdict: LiveVerdict::Skipped(format!(
                        "if_command did not finish within {}s ({cmd})",
                        LIVE_TIMEOUT.as_secs()
                    )),
                };
            }
            Ok(Err(err)) => {
                return BlockReport {
                    index,
                    name,
                    verdict: LiveVerdict::Skipped(format!("if_command could not run: {err}")),
                };
            }
            Ok(Ok(status)) if !status.success() => {
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

    let first = tokio::time::timeout(LIVE_TIMEOUT, async {
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
    let icons = icon_values(widget);
    match widget.get_data(shared, index) {
        Ok(segments) => LiveVerdict::Rendered {
            text: segments
                .iter()
                .map(|s| s.full_text.as_str())
                .collect::<Vec<_>>()
                .join(""),
            icons,
        },
        Err(error) => {
            let mut provided: Vec<String> = widget.values().keys().map(|k| k.to_string()).collect();
            provided.sort_unstable();
            LiveVerdict::RenderError {
                error: error.to_string(),
                provided,
                icons,
            }
        }
    }
}

fn icon_values(widget: &Widget) -> Vec<String> {
    let mut icons: Vec<String> = widget
        .values()
        .values()
        .filter_map(|value| match &value.inner {
            ValueInner::Icon(name, _) => Some(name.to_string()),
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

fn print_block_report(
    report: &BlockReport,
    tag_w: usize,
    out_w: usize,
    style: &Style,
    problems: &mut Vec<Problem>,
    used_now: &mut BTreeMap<String, HashSet<String>>,
) {
    let tag = format!("[{}] {}", report.index + 1, report.name);
    match &report.verdict {
        LiveVerdict::Rendered { text, icons } => {
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
                    .insert(report.name.clone());
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
                    .insert(report.name.clone());
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

#[allow(clippy::too_many_arguments)]
fn print_icon_table(
    base_map: &HashMap<String, Icon>,
    global_overrides: &HashMap<String, Icon>,
    block_overrides: &[(String, HashMap<String, Icon>)],
    builtin: &HashMap<String, Icon>,
    block_names: &[String],
    used_now: &BTreeMap<String, HashSet<String>>,
    font_check: &mut Option<FontCheck>,
    style: &Style,
    problems: &mut Vec<Problem>,
) {
    // may-use: documented icons of each configured block type
    let mut may_use: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for name in block_names {
        if let Ok(i) = BLOCK_ICONS.binary_search_by_key(&name.as_str(), |(block, _)| block) {
            for icon in BLOCK_ICONS[i].1 {
                may_use.entry(icon).or_default().push(name);
            }
        }
    }

    let mut effective: BTreeMap<&str, (&Icon, bool)> = BTreeMap::new();
    for (name, icon) in base_map {
        effective.insert(name, (icon, false));
    }
    for (name, icon) in global_overrides {
        if !base_map.contains_key(name)
            && !builtin.contains_key(name)
            && !used_now.contains_key(name)
            && !may_use.contains_key(name.as_str())
        {
            problems.push(Problem {
                diagnosis: format!(
                    "[icons.overrides] defines {name:?}, which no icon set defines and no \
                     configured block uses (typo?)."
                ),
                fix: None,
            });
        }
        effective.insert(name, (icon, true));
    }

    let mut used_rows: Vec<IconRow> = Vec::new();
    let mut unused: Vec<&str> = Vec::new();
    let mut fallback_rows = 0usize;
    let mut missing_rows = 0usize;

    for (name, (icon, is_override)) in &effective {
        let users_now = used_now.get(*name);
        let users_may = may_use.get(*name);
        if users_now.is_none() && users_may.is_none() {
            unused.push(name);
            continue;
        }
        let mut used_by: Vec<String> = users_now
            .map(|s| s.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        used_by.sort_unstable();
        if let Some(may) = users_may {
            for block in may {
                let as_may = format!("{block} (may)");
                if !used_by.contains(&(*block).to_string()) && !used_by.contains(&as_may) {
                    used_by.push(as_may);
                }
            }
        }
        let used_by = used_by.join(", ");
        let tag = if *is_override { " [override]" } else { "" };

        push_icon_rows(
            &mut used_rows,
            name,
            icon,
            tag,
            &used_by,
            font_check,
            &mut fallback_rows,
            &mut missing_rows,
        );
    }

    // Per-block icons_overrides: these apply only to their block, so they get
    // their own rows tagged with the block name.
    for (block, overrides) in block_overrides {
        let mut names: Vec<&String> = overrides.keys().collect();
        names.sort_unstable();
        for name in names {
            push_icon_rows(
                &mut used_rows,
                name,
                &overrides[name],
                &format!(" [{block} override]"),
                block,
                font_check,
                &mut fallback_rows,
                &mut missing_rows,
            );
        }
    }

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
        problems.push(Problem {
            diagnosis: format!(
                "{fallback_rows} icon glyph(s) will be drawn by fonts outside the bar's font \
                 list (red rows above). Their appearance depends on which fonts happen to be \
                 installed and can change with any font install or system update."
            ),
            fix: Some(
                "Add a font that provides these glyphs to the bar's font directive (matching \
                 the icon set in use), and make sure it is installed."
                    .into(),
            ),
        });
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

    // Names a configured block may request but the current icon map lacks
    for (icon, blocks) in &may_use {
        if !effective.contains_key(icon) {
            problems.push(Problem {
                diagnosis: format!(
                    "Icon {icon:?} may be requested by {} but is not defined by the current icon \
                     set — the block will error when that state occurs.",
                    blocks.join(", ")
                ),
                fix: Some(format!("Add `{icon}` to [icons.overrides].")),
            });
        }
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

fn print_problems(problems: &[Problem], style: &Style) {
    if problems.is_empty() {
        println!("Problems: none found");
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
        Some(Self {
            pattern,
            base_family,
            families,
            cache: HashMap::new(),
        })
    }

    fn check(&mut self, c: char) -> &GlyphFont {
        self.cache.entry(c).or_insert_with(|| {
            let charset = format!("{:x}", c as u32);
            if !fc_list_provides(&charset) {
                return GlyphFont::Missing;
            }
            match fc_match(&self.pattern, Some(&charset)) {
                Some(family) if family == self.base_family => GlyphFont::Base,
                Some(family) => {
                    // fc-match prints a comma-separated family list; if any
                    // member is one of the configured families — by name, or
                    // by what the configured family canonically resolves to
                    // (generic aliases like "monospace" resolve to a concrete
                    // family) — this is the configured fallback doing its
                    // job, not a surprise.
                    let members: Vec<&str> = family.split(',').collect();
                    match self.families.iter().find(|(name, resolved)| {
                        members.contains(&name.as_str())
                            || resolved
                                .as_ref()
                                .is_some_and(|r| r.split(',').any(|m| members.contains(&m)))
                    }) {
                        Some((name, _)) => GlyphFont::Configured(name.clone()),
                        None => GlyphFont::Fallback(family),
                    }
                }
                None => GlyphFont::Missing,
            }
        })
    }
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
/// per-family trailing sizes, split the fallback list on commas.
///
/// `"pango:DejaVu Sans Mono, Font Awesome 6 Free 12"` →
/// `["DejaVu Sans Mono", "Font Awesome 6 Free"]`
fn parse_font_directive(raw: &str) -> Vec<String> {
    let raw = raw.strip_prefix("pango:").unwrap_or(raw);
    raw.split(',')
        .map(strip_font_size)
        .filter(|f| !f.is_empty())
        .collect()
}

/// "DejaVu Sans Mono 13.5" → "DejaVu Sans Mono"
fn strip_font_size(family: &str) -> String {
    let mut parts: Vec<&str> = family.split_whitespace().collect();
    while let Some(last) = parts.last()
        && last.parse::<f64>().is_ok()
    {
        parts.pop();
    }
    parts.join(" ")
}

// ---------------------------------------------------------------------------
// Raw-config format string analysis
// ---------------------------------------------------------------------------

struct FormatUse {
    icon_refs: Vec<String>,
}

struct BlockInfo {
    name: String,
    formats: Vec<FormatUse>,
}

fn collect_blocks(raw: &toml::Value) -> Vec<BlockInfo> {
    let Some(blocks) = raw.get("block").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    blocks
        .iter()
        .map(|block| {
            let name = block
                .get("block")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>")
                .to_string();
            let mut formats = Vec::new();
            collect_formats(block, &mut formats);
            BlockInfo { name, formats }
        })
        .collect()
}

/// Recursively find string values under a `format` or `*_format` key
/// (excluding `icons_format`, which is not a format template) and extract the
/// `^icon_*` references from each.
fn collect_formats(value: &toml::Value, out: &mut Vec<FormatUse>) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (key, value) in table {
        match value {
            toml::Value::String(s)
                if (key == "format" || key.ends_with("_format")) && key != "icons_format" =>
            {
                if let Ok(template) = format_parse::parse_full(s) {
                    let mut icon_refs = Vec::new();
                    collect_icon_refs(&template, &mut icon_refs);
                    if !icon_refs.is_empty() {
                        out.push(FormatUse { icon_refs });
                    }
                }
            }
            toml::Value::Table(_) => collect_formats(value, out),
            toml::Value::Array(array) => {
                for item in array {
                    collect_formats(item, out);
                }
            }
            _ => (),
        }
    }
}

fn collect_icon_refs(template: &format_parse::FormatTemplate, out: &mut Vec<String>) {
    for token_list in &template.0 {
        for token in &token_list.0 {
            match token {
                format_parse::Token::Icon(name) => out.push((*name).to_string()),
                format_parse::Token::Recursive(rec) => collect_icon_refs(rec, out),
                _ => (),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(overrides[0].0, "time");
        assert!(matches!(&overrides[0].1["time"], Icon::Single(s) if s == "CUSTOM"));
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
