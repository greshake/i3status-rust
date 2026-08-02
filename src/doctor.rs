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
            let installed = resolved
                .as_ref()
                .is_some_and(|r| r.split(',').any(|m| family_eq(m, family)));
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
    // ^icon_* references in format strings count as explicit usage
    for (index, info) in collect_blocks(&raw).iter().enumerate() {
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

    match (parsed, skip_live) {
        (None, _) => println!("Blocks: skipped (configuration does not validate)\n"),
        (Some(_), true) => println!("Blocks: skipped (--doctor-skip-live)\n"),
        (Some(config), false) => {
            println!(
                "Blocks (each run for one cycle; performs real requests/commands, {}s timeout)",
                LIVE_TIMEOUT.as_secs()
            );
            let reports = run_live(config_arg, config.blocks.len());
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
    print_icon_table(
        &base_map,
        &global_overrides,
        &block_overrides,
        &builtin,
        &block_names,
        &used_now,
        &mut font_check,
        font_authoritative,
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
fn run_live(config_arg: &str, count: usize) -> Vec<BlockReport> {
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
    unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) };

    let reports = runtime.block_on(async {
        let workers = (0..count).map(|index| {
            let exe = exe.clone();
            async move { run_worker_process(&exe, config_arg, index).await }
        });
        futures::future::join_all(workers).await
    });

    sweep_orphaned_children();
    reports
}

/// Kill every remaining child of this process. All legitimate children (the
/// workers) have already been reaped by this point, so anything left is an
/// escaped descendant of some block's subprocess tree, reparented to us by
/// the subreaper registration.
fn sweep_orphaned_children() {
    let self_pid = std::process::id();
    for _ in 0..20 {
        let mut found = false;
        let Ok(dir) = std::fs::read_dir("/proc") else {
            return;
        };
        for entry in dir.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
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
                // SAFETY: plain syscall
                unsafe { libc::kill(pid as i32, libc::SIGKILL) };
                found = true;
            }
        }
        if !found {
            break;
        }
        // Give the kills a moment: children of the killed processes reparent
        // to us and are caught by the next iteration.
        std::thread::sleep(Duration::from_millis(20));
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

#[allow(clippy::too_many_arguments)]
/// Where a block's icon actually comes from, in precedence order.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
enum Provenance {
    Base,
    Global,
    Local(String),
}

#[allow(clippy::too_many_arguments)]
fn print_icon_table(
    base_map: &HashMap<String, Icon>,
    global_overrides: &HashMap<String, Icon>,
    block_overrides: &[(String, HashMap<String, Icon>)],
    builtin: &HashMap<String, Icon>,
    block_names: &[String],
    used_now: &BTreeMap<String, HashSet<String>>,
    font_check: &mut Option<FontCheck>,
    font_authoritative: bool,
    style: &Style,
    problems: &mut Vec<Problem>,
) {
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
    // each instance's label
    let labels = instance_labels(block_names);
    let mut may_use: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (index, name) in block_names.iter().enumerate() {
        if let Ok(i) = BLOCK_ICONS.binary_search_by_key(&name.as_str(), |(block, _)| block) {
            for icon in BLOCK_ICONS[i].1 {
                may_use
                    .entry(icon)
                    .or_default()
                    .push(labels[index].as_str());
            }
        }
    }

    for name in global_overrides.keys() {
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
            if !users.iter().any(|(b, _)| b == *block) {
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
            match resolve(icon_name, block) {
                Some((_, provenance)) => {
                    let label = if *is_may {
                        format!("{block} (may)")
                    } else {
                        block.clone()
                    };
                    groups.entry(provenance).or_default().push(label);
                }
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
                        None => GlyphFont::Fallback(family),
                    }
                }
                None => GlyphFont::Missing,
            }
        })
    }
}

/// Fontconfig treats family names case-insensitively; compare like it does.
fn family_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
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
