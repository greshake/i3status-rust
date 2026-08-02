use std::process::Command;

/// Extract each block's `# Icons Used` doc section into a static table used
/// by `--doctor` to know which icon names a block may request at runtime.
/// Returns the per-block icon lists for the completeness check.
fn generate_block_icons() -> Vec<(String, Vec<String>)> {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    // The rerun directives replace Cargo's default rerun-on-any-change, so
    // cover everything this script reads (src) AND the git state embedded in
    // VERSION — otherwise a commit could ship a stale hash. `.git` may be a
    // file (linked worktree), so resolve the real git dirs.
    println!("cargo:rerun-if-changed=src");
    emit_git_rerun_directives();

    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    // A missing or unreadable source tree must fail the build: silently
    // generating an empty/partial table would ship a doctor that reports
    // wrong "used by" data.
    let dir = std::fs::read_dir("src/blocks").expect("cannot read src/blocks");
    for entry in dir {
        let entry = entry.expect("cannot read src/blocks directory entry");
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let block = path.file_stem().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
        let mut icons = Vec::new();
        let mut in_section = false;
        for line in source.lines() {
            let Some(doc) = line.trim().strip_prefix("//!") else {
                if in_section {
                    break;
                }
                continue;
            };
            let doc = doc.trim();
            if in_section {
                match doc.strip_prefix("- `").and_then(|d| d.split('`').next()) {
                    Some(icon) => icons.push(icon.to_string()),
                    None if doc.is_empty() => (),
                    None => break,
                }
            } else if let Some(heading) = doc.strip_prefix('#') {
                // Accept the heading variants that exist in the tree:
                // "# Icons Used", "#  Icons Used", "# Used Icons"
                let normalized = heading.split_whitespace().collect::<Vec<_>>().join(" ");
                if normalized.eq_ignore_ascii_case("icons used")
                    || normalized.eq_ignore_ascii_case("used icons")
                {
                    in_section = true;
                }
            }
        }
        // The doc lists are hand-maintained and can lag behind the code, so
        // union them with the icon names passed literally to the icon APIs.
        scan_icon_literals(&source, &mut icons);
        icons.sort();
        icons.dedup();
        if !icons.is_empty() {
            entries.push((block, icons));
        }
    }
    entries.sort();

    let mut code = String::from("pub static BLOCK_ICONS: &[(&str, &[&str])] = &[\n");
    for (block, icons) in &entries {
        code.push_str(&format!("    ({block:?}, &["));
        for icon in icons {
            code.push_str(&format!("{icon:?}, "));
        }
        code.push_str("]),\n");
    }
    code.push_str("];\n");
    std::fs::write(std::path::Path::new(&out_dir).join("block_icons.rs"), code).unwrap();
    entries
}

/// The canonical icon names, scanned from the default map in src/icons.rs
/// (the first quoted string on each `"name" => ...` entry line).
fn canonical_icon_names() -> std::collections::HashSet<String> {
    println!("cargo:rerun-if-changed=src/icons.rs");
    let source = std::fs::read_to_string("src/icons.rs").expect("cannot read src/icons.rs");
    let mut names = std::collections::HashSet::new();
    for line in source.lines() {
        let line = line.trim();
        if !line.starts_with('"') || !line.contains("=>") {
            continue;
        }
        if let Some(name) = line[1..].split('"').next()
            && !name.is_empty()
        {
            names.insert(name.to_string());
        }
    }
    assert!(
        names.len() > 50,
        "canonical icon name scan of src/icons.rs looks broken ({} names)",
        names.len()
    );
    names
}

/// Extract (placeholder key, icon name) pairs from map entries like
/// `"key" => Value::icon("name")`, so doctor can statically tell which icon
/// travels under which placeholder. Helper indirection like
/// `"next" => new_btn("music_next", ...)` is recognized by validating the
/// literal against the canonical icon names. Computed icon names are skipped.
fn generate_block_icon_keys(doc_icons: &[(String, Vec<String>)]) {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let canonical = canonical_icon_names();
    type IconKeyEntry = (
        String,
        Vec<(String, String)>,
        Vec<String>,
        Vec<(String, String)>,
    );
    let mut entries: Vec<IconKeyEntry> = Vec::new();
    let dir = std::fs::read_dir("src/blocks").expect("cannot read src/blocks");
    for entry in dir {
        let entry = entry.expect("cannot read src/blocks directory entry");
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let block = path.file_stem().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
        let mut pairs = Vec::new();
        let mut token_annotated: Vec<String> = Vec::new();
        let mut scopes: Vec<(String, String)> = Vec::new();
        // Explicit associations in the "# Icons Used" doc entries: an entry
        // like `- \`music_play\` (`$play`)` records that the icon travels
        // under the $play placeholder. This covers icon names computed at
        // runtime, which no source scan can attribute.
        for line in source.lines() {
            let Some(doc) = line.trim().strip_prefix("//!") else {
                continue;
            };
            let Some(rest) = doc.trim().strip_prefix("- `") else {
                continue;
            };
            let Some(name) = rest.split('`').next() else {
                continue;
            };
            let mut tail = rest;
            while let Some(pos) = tail.find("`$") {
                tail = &tail[pos + 2..];
                if let Some(key) = tail.split('`').next()
                    && !key.is_empty()
                    && !name.is_empty()
                {
                    pairs.push((key.to_string(), name.to_string()));
                }
            }
            // `^icon_x` annotations mark icons rendered by direct format
            // tokens: they have no placeholder, and the ^icon_* references
            // are analyzed from the format strings themselves.
            if rest.contains("`^icon_") {
                token_annotated.push(name.to_string());
            }
            // backticked format-key tokens scope the icon to those formats
            // (state-specific icons, e.g. bat_charging -> charging_format)
            let mut scope_tail = rest;
            while let Some(pos) = scope_tail.find('`') {
                scope_tail = &scope_tail[pos + 1..];
                let Some(token) = scope_tail.split('`').next() else {
                    break;
                };
                if (token == "format" || token.starts_with("format_") || token.ends_with("_format"))
                    && token.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                {
                    scopes.push((name.to_string(), token.to_string()));
                }
                scope_tail = &scope_tail[token.len()..];
            }
        }
        for line in source.lines() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            let Some(arrow) = line.find("=>") else {
                continue;
            };
            let (before, after) = line.split_at(arrow);
            // key: the last quoted string before the arrow
            let Some(key) = before.rsplit('"').nth(1) else {
                continue;
            };
            // name: the first quoted literal in a call after the arrow
            let Some(paren) = after.find('(') else {
                continue;
            };
            let after = &after[paren + 1..];
            let Some(name) = after.strip_prefix('"').and_then(|a| a.split('"').next()) else {
                continue;
            };
            let direct_icon_call = line.contains("=> Value::icon");
            if !key.is_empty() && !name.is_empty() && (direct_icon_call || canonical.contains(name))
            {
                pairs.push((key.to_string(), name.to_string()));
            }
        }
        pairs.sort();
        pairs.dedup();
        scopes.sort();
        scopes.dedup();
        entries.push((block, pairs, token_annotated, scopes));
    }
    entries.sort();

    // Every documented icon must have at least one placeholder association
    // (from the source scan or a doc annotation); without it the per-icon
    // static analysis in --doctor silently produces wrong results.
    let mut gaps = Vec::new();
    for (block, icons) in doc_icons {
        let (pairs, tokens) = entries
            .iter()
            .find(|(b, ..)| b == block)
            .map(|(_, p, t, _)| (p.as_slice(), t.as_slice()))
            .unwrap_or((&[], &[]));
        for icon in icons {
            if !pairs.iter().any(|(_, name)| name == icon) && !tokens.contains(icon) {
                gaps.push(format!("{block}: {icon}"));
            }
        }
    }
    // Annotation validation: hand-written metadata must not drift. A scope
    // naming a nonexistent format field, or a placeholder key that never
    // appears in the block source, is a typo that would silently suppress
    // or misdirect diagnostics.
    let mut bad_annotations = Vec::new();
    for (block, pairs, _, scopes) in &entries {
        let source = std::fs::read_to_string(format!("src/blocks/{block}.rs")).unwrap_or_default();
        let format_fields: Vec<String> = source
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                let rest = line.strip_prefix("pub ")?;
                let (name, ty) = rest.split_once(':')?;
                let ty = ty.trim().trim_end_matches(',');
                (ty == "FormatConfig" || ty == "Option<FormatConfig>")
                    .then(|| name.trim().to_string())
            })
            .collect();
        for (icon, key) in scopes {
            if !format_fields.iter().any(|f| f == key) {
                bad_annotations.push(format!(
                    "{block}: scope `{key}` on `{icon}` names no FormatConfig field"
                ));
            }
        }
        for (key, icon) in pairs {
            if !source.contains(&format!("\"{key}\"")) && !source.contains(&format!("`${key}`")) {
                bad_annotations.push(format!(
                    "{block}: placeholder `${key}` on `{icon}` never appears in the source"
                ));
            }
        }
    }
    if !bad_annotations.is_empty() {
        panic!(
            "invalid doctor metadata annotations:\n{}",
            bad_annotations.join("\n")
        );
    }

    if !gaps.is_empty() {
        panic!(
            "icons without a placeholder association (annotate their '# Icons Used' doc \
             entry with the placeholder, backtick-dollar-key style):\n{}",
            gaps.join("\n")
        );
    }

    let mut code = String::from("pub static BLOCK_ICON_KEYS: &[(&str, &[(&str, &str)])] = &[\n");
    for (block, pairs, ..) in &entries {
        code.push_str(&format!("    ({block:?}, &["));
        for (key, name) in pairs {
            code.push_str(&format!("({key:?}, {name:?}), "));
        }
        code.push_str("]),\n");
    }
    code.push_str("];\n");
    std::fs::write(
        std::path::Path::new(&out_dir).join("block_icon_keys.rs"),
        code,
    )
    .unwrap();

    // (block, icon) -> format keys the icon is scoped to (empty = all)
    let mut code =
        String::from("pub static BLOCK_ICON_FORMAT_SCOPES: &[(&str, &[(&str, &str)])] = &[\n");
    for (block, _, _, scopes) in &entries {
        if scopes.is_empty() {
            continue;
        }
        code.push_str(&format!("    ({block:?}, &["));
        for (icon, key) in scopes {
            code.push_str(&format!("({icon:?}, {key:?}), "));
        }
        code.push_str("]),\n");
    }
    code.push_str("];\n");
    std::fs::write(
        std::path::Path::new(&out_dir).join("block_icon_format_scopes.rs"),
        code,
    )
    .unwrap();
}

/// Collect string literals passed directly to the icon-requesting APIs.
fn scan_icon_literals(source: &str, icons: &mut Vec<String>) {
    for api in [
        "Value::icon(\"",
        "Value::icon_progression(\"",
        "Value::icon_progression_bound(\"",
    ] {
        let mut rest = source;
        while let Some(pos) = rest.find(api) {
            rest = &rest[pos + api.len()..];
            let Some(end) = rest.find('"') else { break };
            let name = &rest[..end];
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
            {
                icons.push(name.to_string());
            }
        }
    }
}

/// Per-block format-config fields with their defaults, scanned from
/// `pub key: FormatConfig` / `Option<FormatConfig>` declarations and their
/// `.with_default("...")` / `.with_default_format(&other)` uses. A default
/// that cannot be determined is recorded as unknown, which --doctor treats
/// conservatively (as if it could render any icon).
fn generate_block_format_defaults() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let mut entries: Vec<(String, Vec<(String, String, String)>)> = Vec::new();
    let dir = std::fs::read_dir("src/blocks").expect("cannot read src/blocks");
    for entry in dir {
        let entry = entry.expect("cannot read src/blocks directory entry");
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let block = path.file_stem().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));

        // (key, optional)
        let mut fields: Vec<(String, bool)> = Vec::new();
        for line in source.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("pub ") else {
                continue;
            };
            let Some((name, ty)) = rest.split_once(':') else {
                continue;
            };
            let ty = ty.trim().trim_end_matches(',');
            if ty == "FormatConfig" {
                fields.push((name.trim().to_string(), false));
            } else if ty == "Option<FormatConfig>" {
                fields.push((name.trim().to_string(), true));
            }
        }

        let mut rows: Vec<(String, String, String)> = Vec::new();
        for (key, optional) in fields {
            // find `KEY` closely followed by `.with_default`
            let mut spec = ("unknown".to_string(), String::new());
            let mut search = source.as_str();
            while let Some(pos) = search.find(".with_default") {
                let before = &search[..pos];
                // receiver ident, tolerating a multi-line method chain
                let ident: String = before
                    .chars()
                    .rev()
                    .skip_while(|c| c.is_whitespace())
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                // the receiver is the field itself, or a binding whose
                // surrounding context names the field (e.g. memory's
                // `match &config.format_alt { Some(f) => f.with_default(..)`);
                // whitespace is stripped so multi-line chains still match
                let context: String = before[before.len().saturating_sub(160)..]
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                let matches_key = ident == key || context_names_field(&context, &key);
                let after = &search[pos..];
                search = &search[pos + 1..];
                if !matches_key {
                    continue;
                }
                let Some(paren) = after.find('(') else {
                    continue;
                };
                let arg = after[paren + 1..].trim_start();
                if let Some(rest) = arg.strip_prefix('"') {
                    if let Some(end) = rest.find('"') {
                        spec = ("literal".to_string(), rest[..end].to_string());
                        break;
                    }
                } else if let Some(rest) = arg.strip_prefix('&') {
                    let target: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !target.is_empty() {
                        // `&format` binds the local for the `format` field
                        spec = (
                            "inherits".to_string(),
                            target.replace("format_full", "full_format"),
                        );
                        break;
                    }
                }
            }
            let kind = if optional {
                // an Option field that is not configured has NO format at all
                match spec.0.as_str() {
                    "literal" => "optional_literal",
                    "inherits" => "optional_inherits",
                    _ => "optional_unknown",
                }
                .to_string()
            } else {
                spec.0
            };
            rows.push((key, kind, spec.1));
        }
        if !rows.is_empty() {
            rows.sort();
            entries.push((block, rows));
        }
    }
    entries.sort();

    let mut code =
        String::from("pub static BLOCK_FORMAT_DEFAULTS: &[(&str, &[(&str, &str, &str)])] = &[\n");
    for (block, rows) in entries {
        code.push_str(&format!("    ({block:?}, &["));
        for (key, kind, value) in rows {
            code.push_str(&format!("({key:?}, {kind:?}, {value:?}), "));
        }
        code.push_str("]),\n");
    }
    code.push_str("];\n");
    std::fs::write(
        std::path::Path::new(&out_dir).join("block_format_defaults.rs"),
        code,
    )
    .unwrap();
}

/// Whether `context` mentions `config.KEY` as a whole field name (not as a
/// prefix of a longer field like `config.format_alt` for key "format").
fn context_names_field(context: &str, key: &str) -> bool {
    let needle = format!("config.{key}");
    let mut search = context;
    while let Some(pos) = search.find(&needle) {
        let after = &search[pos + needle.len()..];
        if !after
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            return true;
        }
        search = &search[pos + 1..];
    }
    false
}

/// Config keys that rename a canonical icon, scanned from the
/// `config.KEY.as_deref().unwrap_or("canonical")` pattern (e.g. toggle's
/// icon_on/icon_off).
fn generate_block_icon_configs(canonical: &std::collections::HashSet<String>) {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let mut entries: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let dir = std::fs::read_dir("src/blocks").expect("cannot read src/blocks");
    for entry in dir {
        let entry = entry.expect("cannot read src/blocks directory entry");
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let block = path.file_stem().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
        let mut pairs = Vec::new();
        let mut search = source.as_str();
        while let Some(pos) = search.find("config.") {
            let rest = &search[pos + 7..];
            search = rest;
            let key: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let tail = &rest[key.len()..];
            let Some(tail) = tail
                .strip_prefix(".as_deref().unwrap_or(\"")
                .or_else(|| tail.strip_prefix(".as_deref().unwrap_or(r\""))
            else {
                continue;
            };
            let Some(name) = tail.split('"').next() else {
                continue;
            };
            if !key.is_empty() && canonical.contains(name) {
                pairs.push((key.clone(), name.to_string()));
            }
        }
        pairs.sort();
        pairs.dedup();
        if !pairs.is_empty() {
            entries.push((block, pairs));
        }
    }
    entries.sort();

    let mut code = String::from("pub static BLOCK_ICON_CONFIGS: &[(&str, &[(&str, &str)])] = &[\n");
    for (block, pairs) in entries {
        code.push_str(&format!("    ({block:?}, &["));
        for (key, name) in pairs {
            code.push_str(&format!("({key:?}, {name:?}), "));
        }
        code.push_str("]),\n");
    }
    code.push_str("];\n");
    std::fs::write(
        std::path::Path::new(&out_dir).join("block_icon_configs.rs"),
        code,
    )
    .unwrap();
}

/// Watch the git state VERSION embeds, resolving worktree layouts where
/// `.git` is a file and HEAD/refs live in separate git dirs.
fn emit_git_rerun_directives() {
    let dir_of = |args: &[&str]| -> Option<String> {
        let out = Command::new("git")
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let path = String::from_utf8(out.stdout).ok()?.trim().to_string();
        (!path.is_empty()).then_some(path)
    };
    let Some(git_dir) = dir_of(&["rev-parse", "--git-dir"]) else {
        return;
    };
    let common_dir = dir_of(&["rev-parse", "--git-common-dir"]).unwrap_or_else(|| git_dir.clone());
    println!("cargo:rerun-if-changed={git_dir}/HEAD");
    println!("cargo:rerun-if-changed={common_dir}/packed-refs");
    // the branch ref file HEAD points at, when it exists
    if let Ok(head) = std::fs::read_to_string(format!("{git_dir}/HEAD"))
        && let Some(reference) = head.trim().strip_prefix("ref: ")
    {
        println!("cargo:rerun-if-changed={common_dir}/{reference}");
    }
}

fn main() {
    let block_icons = generate_block_icons();
    generate_block_icon_keys(&block_icons);
    generate_block_format_defaults();
    generate_block_icon_configs(&canonical_icon_names());
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .map(|o| String::from_utf8(o.stdout).unwrap());
    let date = Command::new("git")
        .args(["log", "--pretty=format:'%ad'", "-n1", "--date=short"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .map(|o| String::from_utf8(o.stdout).unwrap());
    if let Ok(hash) = hash
        && let Ok(date) = date
    {
        let ver = format!(
            "{} (commit {} {})",
            env!("CARGO_PKG_VERSION"),
            hash.trim(),
            date.trim_matches('\'')
        );
        println!("cargo:rustc-env=VERSION={ver}");
    } else {
        println!("cargo:rustc-env=VERSION={}", env!("CARGO_PKG_VERSION"));
    }
}
