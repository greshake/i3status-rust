use std::io::BufRead;
use std::io::Write;
use std::path::Path;
use std::{env, fs, io};

use clap::ArgMatches;
use clap::CommandFactory;

use anyhow::{Context, Result};

use pandoc::PandocOutput;

enum XTaskSubcommand {
    GenerateManpage,
}

impl TryFrom<&ArgMatches> for XTaskSubcommand {
    type Error = clap::Error;

    fn try_from(value: &ArgMatches) -> Result<Self, Self::Error> {
        if let Some(subcommand) = value.subcommand() {
            match subcommand.0 {
                "generate-manpage" => Ok(XTaskSubcommand::GenerateManpage),
                _ => Err(clap::Error::new(clap::error::ErrorKind::InvalidSubcommand)),
            }
        } else {
            Err(clap::Error::new(clap::error::ErrorKind::MissingSubcommand))
        }
    }
}

fn main() -> Result<()> {
    let arguments_model = clap::Command::new("cargo xtask")
        .version(env!("CARGO_PKG_VERSION"))
        .author("AUTHOR")
        .about("Automation using the cargo xtask convention.")
        .arg_required_else_help(true)
        .bin_name("cargo xtask")
        .disable_version_flag(true)
        .long_about(
            "
Cargo xtask is a convention that allows easy integration of third party commands into the regular
cargo workflow. Xtask's are defined as a separate package and can be used for all kinds of
automation.
        ",
        )
        .subcommand(
            clap::Command::new("generate-manpage")
                .visible_alias("gm")
                .about("Automatic man page generation. Saves the manpage to 'man/i3status-rs.1'"),
        );

    let program_parsed_arguments = arguments_model.get_matches();

    let parsed_subcommand = XTaskSubcommand::try_from(&program_parsed_arguments)?;

    match parsed_subcommand {
        XTaskSubcommand::GenerateManpage => generate_manpage(),
    }
}

fn generate_manpage() -> Result<()> {
    let xtask_manifest_dir = env::var("CARGO_MANIFEST_DIR")?;

    let root_dir = Path::new(&xtask_manifest_dir)
        .parent()
        .context("invalid CARGO_MANIFEST_DIR")?;

    let src_dir = root_dir.join("src");
    let blocks_src_dir = src_dir.join("blocks");
    let man_dir = root_dir.join("man");
    let doc_dir = root_dir.join("doc");
    let man_out_path = man_dir.join("i3status-rs.1");

    let mut result: Vec<_> = fs::read_dir(blocks_src_dir)
        .unwrap()
        .map(Result::unwrap)
        .filter(|e| e.file_type().unwrap().is_file())
        .filter_map(|entry| {
            let block_name = entry
                .file_name()
                .to_str()
                .unwrap()
                .rsplit_once('.')
                .unwrap()
                .0
                .to_string();

            let file = fs::File::open(entry.path()).unwrap();
            let mut doc = String::new();

            for line in io::BufReader::new(file)
                .lines()
                .map(Result::unwrap)
                .take_while(|l| l.starts_with("//!"))
            {
                let mut line = &line[3..];
                if line.starts_with(' ') {
                    line = &line[1..];
                }

                if line.starts_with('#') {
                    doc.push_str("##")
                }
                doc.push_str(line);
                doc.push('\n');
            }

            if !doc.is_empty() {
                Some((block_name, doc))
            } else {
                None
            }
        })
        .collect();

    result.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    let mut md_blocks = String::new();
    for (block, doc) in &result {
        use std::fmt::Write;
        writeln!(md_blocks, "## {block}\n{doc}").unwrap();
    }

    let man_blocks = {
        // pandoc -o man/blocks.1 -t man man/blocks.md
        let mut pandoc = pandoc::new();
        pandoc
            .set_input_format(
                pandoc::InputFormat::Other("markdown-tex_math_dollars".into()),
                vec![],
            )
            .set_input(pandoc::InputKind::Pipe(md_blocks))
            .set_output_format(pandoc::OutputFormat::Man, vec![])
            .set_output(pandoc::OutputKind::Pipe);
        match pandoc.execute()? {
            PandocOutput::ToBuffer(output) => output,
            _ => unreachable!(),
        }
    };

    let man_themes = {
        let md_themes = fs::read_to_string(doc_dir.join("themes.md"))?;
        // pandoc -o man/themes.1 -t man --base-header-level=2 doc/themes.md
        let mut pandoc = pandoc::new();
        pandoc
            .set_input_format(pandoc::InputFormat::Markdown, vec![])
            .set_input(pandoc::InputKind::Pipe(md_themes))
            .set_output_format(pandoc::OutputFormat::Man, vec![])
            .set_output(pandoc::OutputKind::Pipe)
            .add_option(pandoc::PandocOption::ShiftHeadingLevelBy(2));
        match pandoc.execute()? {
            PandocOutput::ToBuffer(output) => output,
            _ => unreachable!(),
        }
    };

    fs::create_dir_all(&man_dir).unwrap();
    let mut out = io::BufWriter::new(fs::File::create(man_out_path).unwrap());
    let man = clap_mangen::Man::new(i3status_rs::CliArgs::command());
    man.render_title(&mut out).unwrap();
    man.render_name_section(&mut out).unwrap();
    man.render_synopsis_section(&mut out).unwrap();
    man.render_description_section(&mut out).unwrap();
    man.render_options_section(&mut out).unwrap();

    out.write_all(b".SH BLOCKS\n")?;
    out.write_all(man_blocks.as_bytes())?;
    out.write_all(b".SH THEMES\n")?;
    out.write_all(man_themes.as_bytes())?;

    man.render_version_section(&mut out).unwrap();
    man.render_authors_section(&mut out).unwrap();

    Ok(())
}
