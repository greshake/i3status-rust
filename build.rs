use std::process::Command;

/// Extract each block's `# Icons Used` doc section into a static table used
/// by `--doctor` to know which icon names a block may request at runtime.
fn generate_block_icons() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    println!("cargo:rerun-if-changed=src/blocks");

    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let Ok(dir) = std::fs::read_dir("src/blocks") else {
        std::fs::write(
            std::path::Path::new(&out_dir).join("block_icons.rs"),
            "pub static BLOCK_ICONS: &[(&str, &[&str])] = &[];\n",
        )
        .unwrap();
        return;
    };
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let block = path.file_stem().unwrap().to_string_lossy().into_owned();
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
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
            } else if doc == "# Icons Used" {
                in_section = true;
            }
        }
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

fn main() {
    generate_block_icons();
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
