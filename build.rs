use std::process::Command;

/// Extract each block's `# Icons Used` doc section into a static table used
/// by `--doctor` to know which icon names a block may request at runtime.
fn generate_block_icons() {
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
    for (block, icons) in entries {
        code.push_str(&format!("    ({block:?}, &["));
        for icon in icons {
            code.push_str(&format!("{icon:?}, "));
        }
        code.push_str("]),\n");
    }
    code.push_str("];\n");
    std::fs::write(std::path::Path::new(&out_dir).join("block_icons.rs"), code).unwrap();
}

/// Extract (placeholder key, icon name) pairs from `"key" => Value::icon*("name")`
/// map entries, so doctor can statically tell which icon travels under which
/// placeholder. Computed icon names are skipped.
fn generate_block_icon_keys() {
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
        for line in source.lines() {
            let Some(arrow) = line.find("=> Value::icon") else {
                continue;
            };
            let (before, after) = line.split_at(arrow);
            // key: the last quoted string before the arrow
            let Some(key) = before.rsplit('"').nth(1) else {
                continue;
            };
            // name: a quoted literal right after the opening parenthesis
            let Some(paren) = after.find('(') else {
                continue;
            };
            let after = &after[paren + 1..];
            let Some(name) = after.strip_prefix('"').and_then(|a| a.split('"').next()) else {
                continue;
            };
            if !key.is_empty() && !name.is_empty() {
                pairs.push((key.to_string(), name.to_string()));
            }
        }
        pairs.sort();
        pairs.dedup();
        if !pairs.is_empty() {
            entries.push((block, pairs));
        }
    }
    entries.sort();

    let mut code = String::from("pub static BLOCK_ICON_KEYS: &[(&str, &[(&str, &str)])] = &[\n");
    for (block, pairs) in entries {
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
    generate_block_icons();
    generate_block_icon_keys();
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
