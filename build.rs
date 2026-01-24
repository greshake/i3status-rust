use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::Command;

fn main() {
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

    let mut builder = phf_codegen::Map::new();

    for line in BufReader::new(File::open("src/themes/xorg-rgb.txt").unwrap()).lines() {
        let line = line.unwrap();
        if line.starts_with("!") {
            continue;
        }
        let mut line_split = line.split_whitespace();
        let r = line_split.next().unwrap();
        let g = line_split.next().unwrap();
        let b = line_split.next().unwrap();
        let name = line_split.collect::<Vec<_>>().join(" ").to_lowercase();
        builder.entry(
            name,
            format!("Color::Rgba(Rgba {{ r: {r}, g: {g}, b: {b}, a: 255 }})"),
        );
    }

    writeln!(
        &mut BufWriter::new(
            File::create(Path::new(&env::var("OUT_DIR").unwrap()).join("xorg_rgb_codegen.rs"))
                .unwrap(),
        ),
        "static XORG_COLORS: ::phf::Map<&'static str, Color> = {};",
        builder.build()
    )
    .unwrap();
}
