//! `--doctor`: diagnose configuration problems.
//!
//! Currently focused on icons, historically the most opaque part of the
//! configuration. The report explains, for everything icon-related, what is
//! used and why:
//!
//! - Which configuration file is used, which locations were searched, and
//!   which candidates are shadowed by earlier ones.
//! - Which icon set file is used, again with the full candidate trace. This
//!   answers "why does `icons = "awesome5"` not work" (the file is looked up
//!   on disk, not built into the binary).
//! - Which icon names a set is missing compared to the built-in set (a named
//!   set *replaces* the built-in one, so a missing name is a runtime error).
//! - Where every override comes from (global `[icons.overrides]` vs a block's
//!   `icons_overrides`), what it replaces, and whether the name looks like a
//!   typo.
//! - Every `^icon_*` reference in format strings, resolved to its final value
//!   and source, flagging references that would error at runtime.
//! - The full effective icon table: every name with its final value and the
//!   layer it came from. Note that there is no fallback between icon sets —
//!   exactly one set is loaded and missing names are runtime errors; the
//!   "my icon was replaced" effect comes from blocks selecting different
//!   names by state, or progression icons selecting a glyph by value.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::*;
use crate::formatting::parse as format_parse;
use crate::icons::{Icon, Icons};
use crate::util;

/// How a single character gets rendered, according to fontconfig.
enum GlyphFont {
    /// Rendered by the primary (first) configured family.
    Base,
    /// Rendered by one of the other configured fallback families — expected
    /// behavior, reported as information only.
    Configured(String),
    /// No configured family has it; fontconfig substitutes this family.
    Fallback(String),
    /// No installed font provides it: renders as an empty box.
    Missing,
}

enum Severity {
    Info,
    Substituted,
    Missing,
}

struct Verdict {
    text: String,
    severity: Severity,
}

/// Asks fontconfig (via `fc-match`/`fc-list`) which font actually renders
/// each glyph. This happens in the bar, outside of i3status-rs, and is the
/// usual source of "my icon was silently replaced by a different symbol":
/// when the bar's font lacks a codepoint, fontconfig substitutes another
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
                    // member is one of the configured families, this is the
                    // configured fallback doing its job, not a surprise.
                    let members: Vec<&str> = family.split(',').collect();
                    match self
                        .families
                        .iter()
                        .find(|(name, _)| members.contains(&name.as_str()))
                    {
                        Some((name, _)) => GlyphFont::Configured(name.clone()),
                        None => GlyphFont::Fallback(family),
                    }
                }
                None => GlyphFont::Missing,
            }
        })
    }

    /// One-line font verdict for an icon's glyphs, or None if every glyph is
    /// rendered by the primary font (the boring case).
    fn icon_verdict(&mut self, icon: &Icon) -> Option<Verdict> {
        let glyphs: Vec<char> = match icon {
            Icon::Single(s) => s.chars().collect(),
            Icon::Progression(v) => v.iter().flat_map(|s| s.chars()).collect(),
        };
        let mut configured: Vec<String> = Vec::new();
        let mut fallbacks: Vec<String> = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        for c in glyphs {
            if c.is_ascii() {
                continue;
            }
            let entry = match self.check(c) {
                GlyphFont::Base => continue,
                GlyphFont::Configured(family) => {
                    (&mut configured, format!("U+{:04X} via {family}", c as u32))
                }
                GlyphFont::Fallback(family) => {
                    (&mut fallbacks, format!("U+{:04X} → {family}", c as u32))
                }
                GlyphFont::Missing => (&mut missing, format!("U+{:04X}", c as u32)),
            };
            if !entry.0.contains(&entry.1) {
                entry.0.push(entry.1);
            }
        }
        if configured.is_empty() && fallbacks.is_empty() && missing.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        let mut severity = Severity::Info;
        if !missing.is_empty() {
            parts.push(format!(
                "✗ no installed font has {}: renders as an empty box",
                missing.join(", ")
            ));
            severity = Severity::Missing;
        }
        if !fallbacks.is_empty() {
            parts.push(format!("⚠ substituted: {}", fallbacks.join(", ")));
            if !matches!(severity, Severity::Missing) {
                severity = Severity::Substituted;
            }
        }
        if !configured.is_empty() {
            parts.push(configured.join(", "));
        }
        Some(Verdict {
            text: parts.join("; "),
            severity,
        })
    }
}

struct DetectedFont {
    tool: &'static str,
    bar_id: String,
    font: String,
}

/// Ask the running i3/sway over IPC which font the bar is configured with,
/// so `--font` is only needed when there is no live bar to ask.
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
                return Some(DetectedFont {
                    tool,
                    bar_id,
                    font: font.to_string(),
                });
            }
        }
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

struct FormatUse {
    key: String,
    icon_refs: Vec<String>,
    parse_error: Option<String>,
}

struct BlockInfo {
    index: usize,
    name: String,
    icons_format: Option<String>,
    overrides: HashMap<String, Icon>,
    formats: Vec<FormatUse>,
}

pub fn run(config_arg: &str, font_arg: Option<&str>) -> Result<()> {
    println!("i3status-rs doctor");
    println!();

    println!("Configuration file (given: {config_arg:?})");
    let Some(config_path) = trace_candidates(config_arg, None) else {
        println!("  ✗ ERROR: no candidate exists — nothing more to check");
        return Ok(());
    };
    println!();

    let raw_text = match std::fs::read_to_string(&config_path) {
        Ok(text) => text,
        Err(err) => {
            println!("✗ ERROR: cannot read {}: {err}", config_path.display());
            return Ok(());
        }
    };
    let raw: toml::Value = match toml::from_str(&raw_text) {
        Ok(value) => value,
        Err(err) => {
            println!("✗ ERROR: configuration is not valid TOML:");
            println!("{err}");
            return Ok(());
        }
    };

    let builtin = Icons::default().0;

    // The icon set fully replaces the built-in map at runtime, so `base_map`
    // is what blocks actually see before overrides.
    let (set_name, overrides_value) = match raw.get("icons") {
        Some(toml::Value::String(name)) => {
            println!(
                "⚠ `icons = {name:?}` at the top level is not valid; use a section:\n  \
                 [icons]\n  icons = {name:?}"
            );
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
    };

    println!("Icon set: {set_name:?}");
    let (base_map, base_source) = if set_name == "none" {
        println!("  using the built-in text icons (\"none\"); no file is read");
        (builtin.clone(), "built-in".to_string())
    } else {
        match trace_candidates_subdir(set_name, Some("icons")) {
            Some(file) => match util::deserialize_toml_file::<HashMap<String, Icon>, _>(&file) {
                Err(err) => {
                    println!("  ✗ ERROR: the file exists but cannot be parsed as an icon set:");
                    println!("    {err}");
                    println!("  (continuing this report with the built-in set)");
                    (builtin.clone(), "built-in (set unparsable)".to_string())
                }
                Ok(map) => {
                    println!("  → {} icons loaded", map.len());

                    let mut missing: Vec<&str> = builtin
                        .keys()
                        .filter(|name| !map.contains_key(*name))
                        .map(String::as_str)
                        .collect();
                    missing.sort_unstable();
                    if missing.is_empty() {
                        println!("  ✓ defines every built-in icon name");
                    } else {
                        println!(
                            "  ⚠ a named icon set REPLACES the built-in set; these {} name(s) are \
                         absent and any block referencing them will error at runtime:",
                            missing.len()
                        );
                        println!("    {}", missing.join(", "));
                    }
                    let source = file.display().to_string();
                    (map, source)
                }
            },
            None => {
                println!("  ✗ ERROR: no candidate exists.");
                println!(
                    "  Icon sets are not built into the binary; they are plain TOML files looked \
                     up on disk."
                );
                println!(
                    "  Fix: copy the set (e.g. files/icons/{set_name}.toml from the source tree) \
                     into ~/.config/i3status-rust/icons/, or use an absolute path in `icons = `."
                );
                println!("  (continuing this report with the built-in set)");
                (builtin.clone(), "built-in (set not found)".to_string())
            }
        }
    };
    println!();

    let global_overrides: HashMap<String, Icon> = match overrides_value {
        Some(value) => match value.clone().try_into() {
            Ok(overrides) => overrides,
            Err(err) => {
                println!(
                    "⚠ [icons.overrides] is not a valid icon table and will be ignored in this \
                     report: {err}\n"
                );
                HashMap::new()
            }
        },
        None => HashMap::new(),
    };

    let blocks = collect_blocks(&raw);
    let referenced: HashSet<&str> = blocks
        .iter()
        .flat_map(|b| b.formats.iter())
        .flat_map(|f| f.icon_refs.iter())
        .map(String::as_str)
        .collect();

    if let Some(icons_format) = raw.get("icons_format").and_then(|v| v.as_str())
        && icons_format != "{icon}"
    {
        println!("Global icons_format: {icons_format:?}");
        println!("  every icon is substituted into this template before display");
        println!();
    }

    println!("How icons are resolved (there is no magic)");
    println!(
        "  1. exactly ONE icon set is loaded (\"none\" = built-in text icons). There is no\n     \
         fallback between sets: a name missing from the set is a runtime error, never\n     \
         silently substituted from another set."
    );
    println!("  2. [icons.overrides], then a block's icons_overrides, replace individual entries.");
    println!(
        "  3. blocks pick WHICH name to display from their own state (e.g. bat vs\n     \
         bat_charging, net_wireless vs net_vpn), and progression icons pick one glyph\n     \
         by value — an \"unexpectedly replaced\" icon is usually a different name or\n     \
         progression step being selected, not a different set."
    );
    println!(
        "  4. i3status-rs only emits text. The bar renders it with pango/fontconfig, and\n     \
         when the bar's font lacks a glyph, fontconfig SILENTLY substitutes another\n     \
         installed font (fc-match) — the usual source of mystery symbols. The table\n     \
         below flags every glyph this happens to."
    );
    println!();

    let detected = if font_arg.is_none() {
        detect_bar_font()
    } else {
        None
    };
    let font_pattern = font_arg.or(detected.as_ref().map(|d| d.font.as_str()));
    let mut font_check = FontCheck::new(font_pattern);
    match &font_check {
        None => {
            println!("Font check: DISABLED — could not run `fc-match` (is fontconfig installed?).");
            println!(
                "  Without it, doctor cannot tell which font will actually draw each icon\n  \
                 glyph in your bar — the most common cause of wrong or inconsistent icons.\n  \
                 Install the fontconfig utilities (package `fontconfig` on most distros)\n  \
                 and re-run."
            );
            println!();
        }
        Some(check) => {
            match (font_arg, &detected) {
                (Some(font), _) => println!("Font check (--font {font:?})"),
                (None, Some(d)) => println!(
                    "Font check (bar font auto-detected via {} from {:?}: {:?})",
                    d.tool, d.bar_id, d.font
                ),
                (None, None) => {
                    println!(
                        "Font check: no --font given, and no running i3/sway answered over IPC,\n  \
                         so doctor is comparing against the fontconfig DEFAULT font — the\n  \
                         substitution column below may not match what your bar really does."
                    );
                    println!(
                        "  For accurate results:\n    \
                         1. find the `font` directive in the `bar {{ ... }}` section of your\n       \
                         i3/sway config (or the top-level `font` if the bar section has none)\n    \
                         2. re-run with it, fallback list and all, e.g.:\n       \
                         i3status-rs --doctor --font \"pango:DejaVu Sans Mono, Font Awesome 6 Free 12\""
                    );
                }
            }
            if font_pattern.is_some_and(|f| f.starts_with('-')) {
                println!(
                    "  ⚠ this looks like an X core font (XLFD), which bypasses fontconfig;\n  \
                     the substitution analysis below is only approximate"
                );
            }
            println!("  plain text renders with: {:?}", check.base_family);
            for (family, resolved) in &check.families {
                match resolved {
                    Some(resolved) if resolved.split(',').any(|m| m == family) => {
                        println!("  ✓ configured family {family:?} is installed");
                    }
                    Some(resolved) => println!(
                        "  ✗ configured family {family:?} is NOT installed — fontconfig \
                         silently uses {resolved:?} in its place"
                    ),
                    None => println!("  ✗ configured family {family:?}: fc-match failed"),
                }
            }
            println!();
        }
    }

    if !global_overrides.is_empty() {
        println!("Global icon overrides ([icons.overrides])");
        let mut names: Vec<&String> = global_overrides.keys().collect();
        names.sort_unstable();
        for name in names {
            let icon = &global_overrides[name];
            match base_map.get(name) {
                Some(previous) => println!(
                    "  {name} = {} (replaces {} from {base_source})",
                    icon_repr(icon),
                    icon_repr(previous)
                ),
                None if builtin.contains_key(name) || referenced.contains(name.as_str()) => {
                    println!(
                        "  {name} = {} (new; not defined by {base_source})",
                        icon_repr(icon)
                    );
                }
                None => println!(
                    "  {name} = {} — ⚠ unknown icon name: not defined by any set and not \
                     referenced by any format string (typo?)",
                    icon_repr(icon)
                ),
            }
        }
        println!();
    }

    // Every name any block can look up globally, with its final value and the
    // layer that value came from.
    let mut effective: HashMap<&str, (&Icon, String)> = HashMap::new();
    for (name, icon) in &base_map {
        effective.insert(name, (icon, base_source.clone()));
    }
    for (name, icon) in &global_overrides {
        effective.insert(name, (icon, "[icons.overrides]".to_string()));
    }
    println!("Effective icon table ({} entries)", effective.len());
    let mut names: Vec<&&str> = effective.keys().collect();
    names.sort_unstable();
    let width = names.iter().map(|n| n.len()).max().unwrap_or(0);
    let mut via_configured = 0usize;
    let mut substituted = 0usize;
    let mut missing_glyphs = 0usize;
    for name in names {
        let (icon, source) = &effective[*name];
        let mut font_note = String::new();
        if let Some(verdict) = font_check
            .as_mut()
            .and_then(|check| check.icon_verdict(icon))
        {
            font_note = format!("  [{}]", verdict.text);
            match verdict.severity {
                Severity::Info => via_configured += 1,
                Severity::Substituted => substituted += 1,
                Severity::Missing => missing_glyphs += 1,
            }
        }
        println!(
            "  {name:<width$}  {}  ← {source}{font_note}",
            icon_repr(icon)
        );
    }
    if font_check.is_some() && via_configured + substituted + missing_glyphs > 0 {
        println!();
        println!(
            "  summary: {via_configured} icon(s) drawn by configured fallback families (fine), \
             {substituted} by fonts NOT in your configuration (⚠), \
             {missing_glyphs} contain glyphs no installed font provides (✗, shown as empty boxes)."
        );
        if substituted + missing_glyphs > 0 {
            println!(
                "  If icons look wrong or inconsistent, substitution is why: install (or pin\n  \
                 in fontconfig) the font your icon set was designed for, and make sure the bar's\n  \
                 font directive selects it."
            );
        }
    }
    println!();

    let interesting: Vec<&BlockInfo> = blocks
        .iter()
        .filter(|b| {
            b.icons_format.is_some()
                || !b.overrides.is_empty()
                || b.formats
                    .iter()
                    .any(|f| !f.icon_refs.is_empty() || f.parse_error.is_some())
        })
        .collect();
    if !interesting.is_empty() {
        println!("Blocks (only those that touch icons are listed)");
        for block in interesting {
            println!("  [[block]] #{} ({})", block.index + 1, block.name);
            if let Some(icons_format) = &block.icons_format {
                println!("    icons_format = {icons_format:?} (wraps every icon of this block)");
            }
            let mut names: Vec<&String> = block.overrides.keys().collect();
            names.sort_unstable();
            for name in names {
                let icon = &block.overrides[name];
                let shadowed = global_overrides
                    .get(name)
                    .map(|icon| (icon, "[icons.overrides]".to_string()))
                    .or_else(|| base_map.get(name).map(|icon| (icon, base_source.clone())));
                match shadowed {
                    Some((previous, source)) => println!(
                        "    icons_overrides: {name} = {} (replaces {} from {source})",
                        icon_repr(icon),
                        icon_repr(previous)
                    ),
                    None => println!(
                        "    icons_overrides: {name} = {} (new; not defined elsewhere)",
                        icon_repr(icon)
                    ),
                }
            }
            for format in &block.formats {
                if let Some(err) = &format.parse_error {
                    println!("    {} — ✗ does not parse: {err}", format.key);
                    continue;
                }
                for name in &format.icon_refs {
                    let resolved = block
                        .overrides
                        .get(name)
                        .map(|icon| (icon, "this block's icons_overrides".to_string()))
                        .or_else(|| {
                            global_overrides
                                .get(name)
                                .map(|icon| (icon, "[icons.overrides]".to_string()))
                        })
                        .or_else(|| base_map.get(name).map(|icon| (icon, base_source.clone())));
                    match resolved {
                        Some((icon, source)) => println!(
                            "    {}: ^icon_{name} → {} (from {source})",
                            format.key,
                            icon_repr(icon)
                        ),
                        None => println!(
                            "    {}: ^icon_{name} — ✗ NOT FOUND: this block will error at runtime",
                            format.key
                        ),
                    }
                }
            }
        }
        println!();
    }

    println!("Config validation");
    match util::deserialize_toml_file::<crate::config::Config, _>(&config_path) {
        Ok(_) => println!("  ✓ the full configuration deserializes cleanly"),
        Err(err) => println!("  ✗ {err}"),
    }

    Ok(())
}

/// Print every path `find_file` would check, marking the one that wins and
/// any existing candidates shadowed by it. Returns the winner.
fn trace_candidates(file: &str, subdir: Option<&str>) -> Option<PathBuf> {
    let mut found: Option<PathBuf> = None;
    for candidate in util::file_candidates(file, subdir, Some("toml")) {
        match (candidate.try_exists(), &found) {
            (Ok(true), None) => {
                println!("  ✓ {} ← using this", candidate.display());
                found = Some(candidate);
            }
            (Ok(true), Some(_)) => {
                println!(
                    "  ✓ {} (exists, but shadowed by the one above)",
                    candidate.display()
                );
            }
            (Ok(false), _) => println!("  ✗ {}", candidate.display()),
            (Err(err), _) => {
                println!("  ? {} (could not check: {err})", candidate.display());
            }
        }
    }
    found
}

fn trace_candidates_subdir(file: &str, subdir: Option<&str>) -> Option<PathBuf> {
    if !Path::new(file).is_absolute() {
        println!("  not an absolute path; searching the standard locations:");
    }
    trace_candidates(file, subdir)
}

fn collect_blocks(raw: &toml::Value) -> Vec<BlockInfo> {
    let Some(blocks) = raw.get("block").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let name = block
                .get("block")
                .and_then(|v| v.as_str())
                .unwrap_or("<missing `block` key>")
                .to_string();
            let icons_format = block
                .get("icons_format")
                .and_then(|v| v.as_str())
                .map(Into::into);
            let overrides = block
                .get("icons_overrides")
                .and_then(|v| v.clone().try_into().ok())
                .unwrap_or_default();
            let mut formats = Vec::new();
            collect_formats(block, "", &mut formats);
            BlockInfo {
                index,
                name,
                icons_format,
                overrides,
                formats,
            }
        })
        .collect()
}

/// Recursively find string values under a `format` or `*_format` key
/// (excluding `icons_format`, which is not a format template) and extract the
/// `^icon_*` references from each.
fn collect_formats(value: &toml::Value, prefix: &str, out: &mut Vec<FormatUse>) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (key, value) in table {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::String(s)
                if (key == "format" || key.ends_with("_format")) && key != "icons_format" =>
            {
                match format_parse::parse_full(s) {
                    Ok(template) => {
                        let mut icon_refs = Vec::new();
                        collect_icon_refs(&template, &mut icon_refs);
                        if !icon_refs.is_empty() {
                            out.push(FormatUse {
                                key: path,
                                icon_refs,
                                parse_error: None,
                            });
                        }
                    }
                    Err(err) => out.push(FormatUse {
                        key: path,
                        icon_refs: Vec::new(),
                        parse_error: Some(err.to_string()),
                    }),
                }
            }
            toml::Value::Table(_) => collect_formats(value, &path, out),
            toml::Value::Array(array) => {
                for (i, item) in array.iter().enumerate() {
                    collect_formats(item, &format!("{path}[{i}]"), out);
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

fn icon_repr(icon: &Icon) -> String {
    match icon {
        Icon::Single(icon) => glyphs_repr(icon),
        Icon::Progression(icons) => format!(
            "[{}]",
            icons
                .iter()
                .map(|s| glyphs_repr(s))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Show the string as-is (whatever the terminal makes of it) plus the
/// codepoints of any non-ASCII characters, so the value is identifiable even
/// when the terminal's font renders it wrong — which is often the very
/// problem being diagnosed.
fn glyphs_repr(s: &str) -> String {
    let non_ascii: Vec<String> = s
        .chars()
        .filter(|c| !c.is_ascii())
        .map(|c| format!("U+{:04X}", c as u32))
        .collect();
    if non_ascii.is_empty() {
        format!("{s:?}")
    } else {
        format!("\"{s}\" ({})", non_ascii.join(" "))
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
    fn format_icon_ref_extraction() {
        let raw: toml::Value = toml::from_str(
            r#"
            [[block]]
            block = "battery"
            format = " ^icon_bat $percentage "
            full_format = " {^icon_bat_charging |}rest "
            icons_format = "not_a_template"
            missing_format = " ^icon_does_not_exist "
            [block.nested]
            some_format = "^icon_nested_ref"

            [[block]]
            block = "time"
            format = " $timestamp "
            "#,
        )
        .unwrap();
        let blocks = collect_blocks(&raw);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].name, "battery");

        let refs: Vec<&str> = blocks[0]
            .formats
            .iter()
            .flat_map(|f| f.icon_refs.iter().map(String::as_str))
            .collect();
        assert!(refs.contains(&"bat"));
        assert!(refs.contains(&"bat_charging"));
        assert!(refs.contains(&"does_not_exist"));
        assert!(refs.contains(&"nested_ref"));
        // icons_format is not a format template and must not be parsed as one
        assert!(!blocks[0].formats.iter().any(|f| f.key == "icons_format"));

        // no icon references at all -> no format entries recorded
        assert!(blocks[1].formats.is_empty());
    }

    #[test]
    fn unparsable_format_is_reported_not_fatal() {
        let raw: toml::Value = toml::from_str(
            r#"
            [[block]]
            block = "custom"
            format = " $unclosed.str(w:"
            "#,
        )
        .unwrap();
        let blocks = collect_blocks(&raw);
        assert_eq!(blocks[0].formats.len(), 1);
        assert!(blocks[0].formats[0].parse_error.is_some());
    }

    #[test]
    fn glyph_representation() {
        assert_eq!(glyphs_repr("BAT"), "\"BAT\"");
        assert_eq!(glyphs_repr("\u{f244}"), "\"\u{f244}\" (U+F244)");
        assert_eq!(glyphs_repr("🍅"), "\"🍅\" (U+1F345)");
    }
}
