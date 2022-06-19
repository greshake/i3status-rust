use std::env::args;
use std::fs::{read_dir, File};
use std::io::{BufRead, BufReader, Result, Write};
use std::path::PathBuf;
use std::process::{exit, Command};
use tempfile::NamedTempFile;

const USAGE: &str = concat!(
    "i3status-rust manpage generator\n",
    "\n",
    "USAGE:\n",
    "  gen-manpage <i3status-rs str dir> <output dir>\n",
    "EXAMPLE:\n",
    "  gen-manpage ../src/blocks ../man\n",
);

fn main() {
    let mut args = args();
    let _ = args.next().unwrap();
    let src_dir = match args.next() {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("{USAGE}");
            exit(1);
        }
    };
    let out_dir = match args.next() {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("{USAGE}");
            exit(1);
        }
    };

    let blocks_dir = src_dir.join("blocks");

    let mut result = Vec::new();

    for entry in read_dir(&blocks_dir)
        .unwrap()
        .map(Result::unwrap)
        .filter(|e| e.file_type().unwrap().is_file())
    {
        let block_name = entry
            .file_name()
            .to_str()
            .unwrap()
            .rsplit_once('.')
            .unwrap()
            .0
            .to_string();

        let file = File::open(blocks_dir.join(entry.file_name())).unwrap();
        let mut doc = String::new();

        for line in BufReader::new(file)
            .lines()
            .map(Result::unwrap)
            .take_while(|l| l.starts_with("//!"))
        {
            let mut line = &line[3..];
            if line.starts_with(' ') {
                line = &line[1..];
            }

            if line.starts_with("#") {
                doc.push_str("##")
            }
            doc.push_str(line);
            doc.push('\n');
        }

        if !doc.is_empty() {
            result.push((block_name, doc));
        }
    }

    result.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    let mut markdown = NamedTempFile::new().unwrap();
    for (block, _) in &result {
        writeln!(markdown, "- [{block}]({block})").unwrap();
    }
    for (block, doc) in &result {
        writeln!(markdown, "- ## {block}\n{doc}").unwrap();
    }
    markdown.flush().unwrap();

    Command::new("pandoc")
        .arg("-o")
        .arg(out_dir.join("blocks.1"))
        .arg("-f")
        .arg("markdown")
        .arg("-t")
        .arg("man")
        .arg(markdown.path())
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    drop(markdown);
}
