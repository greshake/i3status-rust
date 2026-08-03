use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

struct FixtureDir(PathBuf);

impl FixtureDir {
    fn new(name: &str) -> Self {
        let serial = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "i3status-rs-doctor-{name}-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("create doctor fixture directory");
        Self(path)
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, contents).expect("write doctor fixture");
        path
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn doctor(config: &Path, skip_live: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_i3status-rs"));
    command.arg("--doctor").arg("--font").arg("monospace");
    if skip_live {
        command.arg("--doctor-skip-live");
    }
    command
        .arg(config)
        .env_remove("I3RS_DBUS_NAME")
        .output()
        .expect("run doctor")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn toml_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn fontconfig_can_prove_missing(codepoint: &str) -> bool {
    let Ok(version) = Command::new("fc-list").arg("--version").output() else {
        return false;
    };
    if !version.status.success() {
        return false;
    }
    Command::new("fc-list")
        .arg(format!(":charset={codepoint}"))
        .arg("family")
        .output()
        .is_ok_and(|output| output.status.success() && output.stdout.is_empty())
}

fn command_is_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[test]
fn undefined_declared_icon_is_a_problem_without_a_live_run() {
    let fixture = FixtureDir::new("undefined-declaration");
    let icons = fixture.write("icons.toml", "");
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " load "

[icons]
icons = "{}"
"#,
            toml_path(&icons)
        ),
    );

    let output = doctor(&config, true);
    assert!(
        !output.status.success(),
        "a declared icon with no definition must fail doctor:\n{}",
        stdout(&output)
    );
    assert!(stdout(&output).contains("cogs (load)"));
}

#[test]
fn empty_declared_icon_progression_is_a_problem_without_a_live_run() {
    let fixture = FixtureDir::new("empty-declaration");
    let icons = fixture.write("icons.toml", "cogs = []\n");
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " load "

[icons]
icons = "{}"
"#,
            toml_path(&icons)
        ),
    );

    let output = doctor(&config, true);
    assert!(
        !output.status.success(),
        "an empty progression cannot satisfy a declared icon obligation:\n{}",
        stdout(&output)
    );
    assert!(stdout(&output).contains("cogs (load)"));
}

#[test]
fn skipped_block_keeps_its_declared_icon_obligations() {
    let fixture = FixtureDir::new("skipped-declaration");
    let icons = fixture.write("icons.toml", "");
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
if_command = "false"
format = " load "

[icons]
icons = "{}"
"#,
            toml_path(&icons)
        ),
    );

    let output = doctor(&config, false);
    assert!(stdout(&output).contains("(skipped:"));
    assert!(
        !output.status.success(),
        "a transient if_command result must not erase static obligations:\n{}",
        stdout(&output)
    );
    assert!(stdout(&output).contains("cogs (load)"));
}

#[test]
fn short_format_protocol_sentinel_is_not_reported_as_output_or_pango() {
    let fixture = FixtureDir::new("short-sentinel");
    let icons = fixture.write("icons.toml", "cogs = \"COGS\"\n");
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = {{ full = " load ", short = " short " }}

[icons]
icons = "{}"
"#,
            toml_path(&icons)
        ),
    );

    let output = doctor(&config, false);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(
        !stdout.contains("<span/>"),
        "the i3bar short-text sentinel is not rendered user output:\n{stdout}"
    );
    assert!(
        !stdout.contains("pango markup (from a format, icons_format, or an icon value) affects"),
        "the internal short-text sentinel must not make font checks inconclusive:\n{stdout}"
    );
}

#[test]
fn short_text_glyphs_are_font_checked() {
    if !fontconfig_can_prove_missing("10ffff") {
        eprintln!("skipping: fc-list cannot prove U+10FFFF is unavailable");
        return;
    }

    let fixture = FixtureDir::new("short-glyph");
    let icons = fixture.write("icons.toml", "cogs = \"COGS\"\n");
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = {{ full = " load ", short = " 􏿿 " }}

[icons]
icons = "{}"
"#,
            toml_path(&icons)
        ),
    );

    let output = doctor(&config, false);
    assert!(
        !output.status.success(),
        "a missing glyph in short output must be diagnosed:\n{}",
        stdout(&output)
    );
    assert!(stdout(&output).contains("U+10FFFF"));
}

#[test]
fn global_error_format_markup_makes_its_icons_inconclusive() {
    let fixture = FixtureDir::new("global-error-markup");
    let icons = fixture.write("icons.toml", "cogs = \"COGS\"\nrefresh = \"R\"\n");
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
error_format = " <span font_family='Example'>$restart_block_icon</span> $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " load "
max_retries = 0

[icons]
icons = "{}"
"#,
            toml_path(&icons)
        ),
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout
            .contains("pango markup (from a format, icons_format, or an icon value) affects: load"),
        "global effective error formats must participate in markup scoping:\n{stdout}"
    );
}

#[test]
fn unrelated_icon_definition_does_not_exempt_a_live_text_glyph() {
    if !fontconfig_can_prove_missing("10ffff") {
        eprintln!("skipping: fc-list cannot prove U+10FFFF is unavailable");
        return;
    }

    let fixture = FixtureDir::new("text-icon-collision");
    let icons = fixture.write("icons.toml", "unused = \"􏿿\"\n");
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom"
command = '''printf '\364\217\277\277\n' '''
interval = "once"

[icons]
icons = "{}"
"#,
            toml_path(&icons)
        ),
    );

    let output = doctor(&config, false);
    assert!(
        !output.status.success(),
        "an unused icon with the same character must not hide missing live text:\n{}",
        stdout(&output)
    );
    assert!(stdout(&output).contains("U+10FFFF"));
}

#[test]
fn mixed_text_run_attributes_only_the_missing_codepoint() {
    if !fontconfig_can_prove_missing("10ffff") {
        eprintln!("skipping: fc-list cannot prove U+10FFFF is unavailable");
        return;
    }

    let fixture = FixtureDir::new("mixed-text-run");
    let icons = fixture.write("icons.toml", "");
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom"
command = '''printf '\303\251\364\217\277\277\n' '''
interval = "once"

[icons]
icons = "{}"
"#,
            toml_path(&icons)
        ),
    );

    let output = doctor(&config, false);
    let stdout = stdout(&output);
    assert!(!output.status.success(), "{stdout}");
    let diagnosis = stdout
        .lines()
        .find(|line| line.contains("no installed font provides"))
        .expect("missing-glyph diagnosis");
    assert!(diagnosis.contains("U+10FFFF"), "{diagnosis}");
    assert!(
        !diagnosis.contains("U+00E9"),
        "the available é must not be attributed to the missing provider: {diagnosis}"
    );
}

#[test]
fn live_output_control_characters_are_escaped() {
    let fixture = FixtureDir::new("control-output");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom"
command = '''printf '%s\n' '{"text":"first\nsecond\u001b[31mRED"}' '''
json = true
interval = "once"
"#,
    );

    let output = doctor(&config, false);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(
        !stdout.contains('\u{1b}'),
        "configured output must not inject terminal control sequences:\n{stdout:?}"
    );
    assert!(
        stdout.contains(r"first\nsecond"),
        "embedded newlines should be displayed in escaped form:\n{stdout:?}"
    );
}

#[test]
fn simultaneous_doctor_runs_do_not_share_a_dbus_worker_name() {
    if !command_is_available("dbus-run-session") || !command_is_available("busctl") {
        eprintln!("skipping: dbus-run-session and busctl are required");
        return;
    }

    let fixture = FixtureDir::new("dbus-isolation");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom_dbus"
path = "/doctor_test"
format = " $text "
"#,
    );
    let first_report = fixture.0.join("first.json");
    let second_report = fixture.0.join("second.json");

    // Both workers use index zero, just as two independent Doctor processes
    // would. A private session bus makes the collision test deterministic.
    let script = r#"
"$1" --doctor-worker 0 "$2" >"$3" &
first=$!
"$1" --doctor-worker 0 "$2" >"$4" &
second=$!

# Wait for both workers to register, then make every Doctor-owned service
# publish its first widget so the workers can exit normally.
attempt=0
while [ "$attempt" -lt 20 ]; do
    names=$(busctl --user --no-pager --no-legend list | awk '$1 ~ /^rs\.i3status/ { print $1 }')
    count=$(printf '%s\n' "$names" | awk 'NF { count++ } END { print count + 0 }')
    [ "$count" -ge 2 ] && break
    attempt=$((attempt + 1))
    sleep 0.05
done
for name in $names; do
    busctl --user call "$name" /doctor_test rs.i3status.custom SetText ss ready ready >/dev/null
done

wait "$first"
wait "$second"
"#;
    let session = Command::new("dbus-run-session")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(script)
        .arg("doctor-dbus-test")
        .arg(env!("CARGO_BIN_EXE_i3status-rs"))
        .arg(&config)
        .arg(&first_report)
        .arg(&second_report)
        .output()
        .expect("run workers in a private D-Bus session");
    assert!(
        session.status.success(),
        "private D-Bus session failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );

    let first = std::fs::read_to_string(&first_report).expect("read first worker report");
    let second = std::fs::read_to_string(&second_report).expect("read second worker report");
    let is_rendered = |report: &str| {
        serde_json::from_str::<serde_json::Value>(report)
            .ok()
            .and_then(|report| report.get("verdict")?.get("Rendered").cloned())
            .is_some()
    };
    assert!(
        is_rendered(&first) && is_rendered(&second),
        "simultaneous index-zero workers must both acquire distinct names:\nfirst: {first}\nsecond: {second}"
    );
}
