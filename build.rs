use std::process::Command;

/// Extract each block's `# Icons Used` doc section into a static table used
/// by `--doctor` to know which icon names a block may request at runtime.
/// Returns the per-block icon lists for the completeness check.
fn generate_block_icons() -> Vec<(String, Vec<String>)> {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    println!("cargo:rerun-if-changed=src/blocks");

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
    let mut entries: Vec<(String, Vec<(String, String)>, Vec<String>)> = Vec::new();
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
        entries.push((block, pairs, token_annotated));
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
            .map(|(_, p, t)| (p.as_slice(), t.as_slice()))
            .unwrap_or((&[], &[]));
        for icon in icons {
            if !pairs.iter().any(|(_, name)| name == icon) && !tokens.contains(icon) {
                gaps.push(format!("{block}: {icon}"));
            }
        }
    }
    if !gaps.is_empty() {
        panic!(
            "icons without a placeholder association (annotate their '# Icons Used' doc \
             entry with the placeholder, backtick-dollar-key style):\n{}",
            gaps.join("\n")
        );
    }

    let mut code = String::from("pub static BLOCK_ICON_KEYS: &[(&str, &[(&str, &str)])] = &[\n");
    for (block, pairs, _) in entries {
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

fn main() {
    let block_icons = generate_block_icons();
    generate_block_icon_keys(&block_icons);
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
