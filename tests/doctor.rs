use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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

    #[cfg(unix)]
    fn write_executable(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.write(name, contents);
        let mut permissions = std::fs::metadata(&path)
            .expect("read fixture executable metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("make fixture executable");
        path
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn doctor(config: &Path, skip_live: bool) -> Output {
    doctor_with_font(config, skip_live, "monospace")
}

fn doctor_with_font(config: &Path, skip_live: bool, font: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_i3status-rs"));
    command.arg("--doctor").arg("--font").arg(font);
    if skip_live {
        command.arg("--doctor-skip-live");
    }
    command
        .arg(config)
        .env_remove("I3RS_DBUS_NAME")
        .output()
        .expect("run doctor")
}

#[cfg(unix)]
fn path_with_front(path: &Path) -> std::ffi::OsString {
    let mut paths = vec![path.to_path_buf()];
    if let Some(current) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&current));
    }
    std::env::join_paths(paths).expect("construct fixture PATH")
}

fn font_family_lacks_codepoint(family: &str, codepoint: &str) -> bool {
    let query = |pattern: &str| {
        Command::new("fc-list")
            .arg(pattern)
            .arg("family")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| !output.stdout.is_empty())
    };
    query(family) == Some(true)
        && query(&format!("{family}:charset={codepoint}")) == Some(false)
        && query(&format!(":charset={codepoint}")) == Some(true)
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
fn live_worker_accepts_a_config_name_that_starts_with_a_dash() {
    let fixture = FixtureDir::new("dash-prefixed-config");
    let config_dir = fixture.0.join("i3status-rust");
    std::fs::create_dir(&config_dir).expect("create XDG config directory");
    std::fs::write(
        config_dir.join("--doctor-fixture.toml"),
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom"
command = "printf ok"
interval = "once"
"#,
    )
    .expect("write dash-prefixed config");

    let output = Command::new(env!("CARGO_BIN_EXE_i3status-rs"))
        .arg("--doctor")
        .arg("--font")
        .arg("monospace")
        .arg("--")
        .arg("--doctor-fixture")
        .env("XDG_CONFIG_HOME", &fixture.0)
        .env_remove("I3RS_DBUS_NAME")
        .output()
        .expect("run doctor with a dash-prefixed config name");

    assert!(
        output.status.success(),
        "the worker must preserve the parent's end-of-options boundary:\n{}",
        stdout(&output)
    );
}

#[cfg(unix)]
#[test]
fn config_search_errors_are_not_skipped_for_lower_priority_files() {
    use std::os::unix::fs::symlink;

    let fixture = FixtureDir::new("config-search-error");
    let config_home = fixture.0.join("config/i3status-rust");
    let data_home = fixture.0.join("data/i3status-rust");
    std::fs::create_dir_all(&config_home).expect("create XDG config directory");
    std::fs::create_dir_all(&data_home).expect("create XDG data directory");

    // The real bar stops at this ELOOP error. Doctor must not continue and
    // claim the lower-priority XDG_DATA_HOME configuration is usable.
    symlink("config.toml", config_home.join("config.toml"))
        .expect("create self-referential config symlink");
    std::fs::write(
        data_home.join("config.toml"),
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "time"
format = " clock "
"#,
    )
    .expect("write lower-priority config");

    let output = Command::new(env!("CARGO_BIN_EXE_i3status-rs"))
        .arg("--doctor")
        .arg("--doctor-skip-live")
        .arg("--font")
        .arg("monospace")
        .arg("config.toml")
        .env("XDG_CONFIG_HOME", fixture.0.join("config"))
        .env("XDG_DATA_HOME", fixture.0.join("data"))
        .env_remove("I3RS_DBUS_NAME")
        .output()
        .expect("run doctor with a failing higher-priority candidate");

    let stdout = stdout(&output);
    assert!(
        !output.status.success(),
        "doctor must fail when the bar's own file search would fail:\n{stdout}"
    );
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

#[test]
fn format_alt_pango_markup_is_inconclusive_without_rendering_that_variant() {
    let fixture = FixtureDir::new("format-alt-markup");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "cpu"
format = " $icon "
format_alt = " <span font_family='Example'>$icon</span> "
"#,
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout
            .contains("pango markup (from a format, icons_format, or an icon value) affects: cpu"),
        "every configured format variant must participate in markup scoping:\n{stdout}"
    );
}

#[test]
fn pango_entity_in_format_is_inconclusive() {
    let fixture = FixtureDir::new("format-entity-markup");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " &#x10FFFF; $icon "
"#,
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout
            .contains("pango markup (from a format, icons_format, or an icon value) affects: load"),
        "Pango character references are markup even without a '<' tag:\n{stdout}"
    );
}

#[test]
fn pango_entity_in_icons_format_is_inconclusive() {
    let fixture = FixtureDir::new("icons-format-entity-markup");
    let config = fixture.write(
        "config.toml",
        r#"
icons_format = "&#x10FFFF;{icon}"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " $icon "
"#,
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout
            .contains("pango markup (from a format, icons_format, or an icon value) affects: load"),
        "Pango character references in icons_format make the effective glyph inconclusive:\n{stdout}"
    );
}

#[test]
fn pango_entity_in_icon_value_is_inconclusive() {
    let fixture = FixtureDir::new("icon-entity-markup");
    let icons = fixture.write("icons.toml", "cogs = \"&#x10FFFF;\"\n");
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " $icon "

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
        stdout.contains("(depends on pango markup)"),
        "an icon value can contain Pango markup without containing a '<' tag:\n{stdout}"
    );
}

#[test]
fn icons_format_added_glyphs_are_validated_in_static_and_live_modes() {
    if !fontconfig_can_prove_missing("10ffff") {
        eprintln!("skipping: fc-list cannot prove U+10FFFF is unavailable");
        return;
    }

    let fixture = FixtureDir::new("icons-format-glyph");
    let icons = fixture.write("icons.toml", "cogs = \"COGS\"\n");
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
icons_format = "{{icon}}􏿿"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " $icon "

[icons]
icons = "{}"
"#,
            toml_path(&icons)
        ),
    );

    let failures: Vec<String> = [true, false]
        .into_iter()
        .filter_map(|skip_live| {
            let output = doctor(&config, skip_live);
            let stdout = stdout(&output);
            (output.status.success() || !stdout.contains("U+10FFFF")).then(|| {
                format!("skip_live={skip_live} did not diagnose the icons_format glyph:\n{stdout}")
            })
        })
        .collect();
    assert!(
        failures.is_empty(),
        "icons_format output is part of every rendered icon:\n{}",
        failures.join("\n")
    );
}

#[test]
fn open_icon_contract_is_disclosed_as_partial() {
    let fixture = FixtureDir::new("open-icon-contract");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom"
command = '''printf '%s\n' '{"text":"ready"}' '''
json = true
interval = "once"
"#,
    );

    let output = doctor(&config, false);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout}");
    let lower = stdout.to_lowercase();
    assert!(
        lower.contains("dynamic icon") && lower.contains("custom"),
        "a clean live sample must disclose an explicitly open icon contract:\n{stdout}"
    );
}

#[test]
fn render_error_control_characters_are_escaped() {
    let fixture = FixtureDir::new("control-render-error");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom"
command = '''printf '%s\n' '{"text":"x","icon":"\u001b[31mBAD"}' '''
json = true
interval = "once"
"#,
    );

    let output = doctor(&config, false);
    let stdout = stdout(&output);
    assert!(!output.status.success(), "{stdout}");
    assert!(
        !stdout.contains('\u{1b}'),
        "a render error derived from block output must not inject terminal controls:\n{stdout:?}"
    );
}

#[test]
fn icon_table_control_characters_are_escaped() {
    let fixture = FixtureDir::new("control-icon-table");
    let icons = fixture.write("icons.toml", r#"cogs = "first\n\u001b[31mRED""#);
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " $icon "

[icons]
icons = "{}"
"#,
            toml_path(&icons)
        ),
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout:?}");
    assert!(
        !stdout.contains('\u{1b}'),
        "an icon value must not inject terminal controls into its table row:\n{stdout:?}"
    );
    assert!(
        stdout.contains(r"first\n"),
        "icon-table newlines should be displayed in escaped form:\n{stdout:?}"
    );
}

#[test]
fn icon_table_name_control_characters_are_escaped() {
    let fixture = FixtureDir::new("control-icon-name");
    let icons = fixture.write("icons.toml", r#""\u001b[31mBAD" = "SAFE""#);
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom"
command = '''printf '%s\n' '{{"text":"x","icon":"\u001b[31mBAD"}}' '''
json = true
interval = "once"

[icons]
icons = "{}"
"#,
            toml_path(&icons)
        ),
    );

    let output = doctor(&config, false);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout:?}");
    assert!(
        !stdout.contains('\u{1b}'),
        "an icon name must not inject terminal controls into its table row:\n{stdout:?}"
    );
}

#[test]
fn missing_icon_name_with_an_apostrophe_is_reported_once() {
    let fixture = FixtureDir::new("apostrophe-icon-name");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom"
command = '''printf '%s\n' '{"text":"x","icon":"can'\''t"}' '''
json = true
interval = "once"
"#,
    );

    let output = doctor(&config, false);
    let stdout = stdout(&output);
    assert!(!output.status.success(), "{stdout}");
    assert!(
        stdout.contains("Problems (1)"),
        "one missing icon must produce one diagnosis even when its name contains an apostrophe:\n{stdout}"
    );
}

#[test]
fn text_glyph_diagnostics_escape_control_characters() {
    let fixture = FixtureDir::new("control-text-diagnostic");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom"
command = '''printf '%s\n' '{"text":"\u0085"}' '''
json = true
interval = "once"
"#,
    );

    let output = doctor(&config, false);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout:?}");
    assert!(
        !stdout.contains('\u{85}'),
        "a live text glyph must not inject a control character through a font diagnostic:\n{stdout:?}"
    );
}

#[test]
fn ascii_icon_glyphs_are_font_checked() {
    const FONT: &str = "Font Awesome 5 Free";
    if !font_family_lacks_codepoint(FONT, "21") {
        eprintln!("skipping: {FONT} must be installed without U+0021 for this regression test");
        return;
    }

    let fixture = FixtureDir::new("ascii-icon-font");
    let icons = fixture.write("icons.toml", "cogs = \"!\"\n");
    let config = fixture.write(
        "config.toml",
        &format!(
            r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " $icon "

[icons]
icons = "{}"
"#,
            toml_path(&icons)
        ),
    );

    let output = doctor_with_font(&config, true, FONT);
    let stdout = stdout(&output);
    assert!(
        !output.status.success(),
        "a printable ASCII glyph missing from the configured font must be diagnosed:\n{stdout}"
    );
    assert!(stdout.contains("outside the bar's font list"), "{stdout}");
}

#[test]
fn ascii_text_glyphs_are_font_checked() {
    const FONT: &str = "Font Awesome 5 Free";
    if !font_family_lacks_codepoint(FONT, "21") {
        eprintln!("skipping: {FONT} must be installed without U+0021 for this regression test");
        return;
    }

    let fixture = FixtureDir::new("ascii-text-font");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom"
command = "printf '!\\n'"
interval = "once"
"#,
    );

    let output = doctor_with_font(&config, false, FONT);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("Text glyphs in live output drawn by fonts outside the bar's font list"),
        "printable ASCII text must be checked for font substitution too:\n{stdout}"
    );
}

#[test]
fn invalid_global_icon_overrides_are_reported_once() {
    let fixture = FixtureDir::new("invalid-global-overrides");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " load "

[icons]
overrides = "not a table"
"#,
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(!output.status.success(), "{stdout}");
    assert!(
        stdout.contains("Problems (1)"),
        "one malformed overrides value must not be reported as two configuration problems:\n{stdout}"
    );
}

#[test]
fn invalid_per_block_icon_overrides_are_reported_once() {
    let fixture = FixtureDir::new("invalid-block-overrides");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " load "
icons_overrides = "not a table"
"#,
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(!output.status.success(), "{stdout}");
    assert!(
        stdout.contains("Problems (1)"),
        "one malformed per-block override must not produce both generic and specific diagnoses:\n{stdout}"
    );
}

#[test]
fn icon_set_problem_does_not_suppress_an_unrelated_config_error() {
    let fixture = FixtureDir::new("independent-structural-errors");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "
icons_format = 42

[[block]]
block = "load"
format = " load "

[icons]
icons = "definitely-not-an-installed-icon-set"
"#,
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(!output.status.success(), "{stdout}");
    assert!(stdout.contains("Icon set"), "{stdout}");
    assert!(
        stdout.contains("Configuration does not validate"),
        "an icon-set lookup failure must not hide an independent typed-config error:\n{stdout}"
    );
    assert!(stdout.contains("Problems (2)"), "{stdout}");
}

#[test]
fn missing_icon_set_is_reported_once() {
    let fixture = FixtureDir::new("missing-icon-set-once");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " load "

[icons]
icons = "definitely-not-an-installed-icon-set"
"#,
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(!output.status.success(), "{stdout}");
    assert!(
        stdout.contains("Problems (1)"),
        "a missing icon set must not produce both a specific and generic diagnosis:\n{stdout}"
    );
}

#[test]
fn skipped_live_test_does_not_claim_dynamic_icons_were_observed() {
    let fixture = FixtureDir::new("skipped-open-contract");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom"
command = "true"
json = true
interval = "once"
"#,
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("dynamic icon source"), "{stdout}");
    assert!(
        !stdout.contains("what this run"),
        "--doctor-skip-live must not claim that a run observed dynamic icon names:\n{stdout}"
    );
}

#[test]
fn toml_parse_errors_escape_control_characters() {
    let fixture = FixtureDir::new("control-toml-error");
    let config = fixture.write("config.toml", "\u{1b}[31m = invalid\n");

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(!output.status.success(), "{stdout:?}");
    assert!(
        !stdout.contains('\u{1b}'),
        "source excerpts in TOML diagnostics must not inject terminal controls:\n{stdout:?}"
    );
}

#[test]
fn reported_file_paths_escape_control_characters() {
    let fixture = FixtureDir::new("control-paths");
    let icons = fixture.write("icons-\u{1b}[31m.toml", "cogs = \"COGS\"\n");
    let escaped_icons_path = toml_path(&icons).replace('\u{1b}', r"\u001b");
    let config = fixture.write(
        "config-\u{1b}[31m.toml",
        &format!(
            r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " load "

[icons]
icons = "{escaped_icons_path}"
"#,
        ),
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout:?}");
    assert!(
        !stdout.contains('\u{1b}'),
        "config and icon-set paths must not inject terminal controls into the report:\n{stdout:?}"
    );
}

#[test]
fn missing_icon_search_trace_escapes_control_characters() {
    let fixture = FixtureDir::new("control-missing-icon-path");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " load "

[icons]
icons = "\u001b[31mmissing"
"#,
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(!output.status.success(), "{stdout:?}");
    assert!(stdout.contains("not found"), "{stdout:?}");
    assert!(
        !stdout.contains('\u{1b}'),
        "a missing icon set's search trace must not inject terminal controls:\n{stdout:?}"
    );
}

#[test]
fn missing_icon_search_trace_escapes_embedded_newlines() {
    let fixture = FixtureDir::new("newline-missing-icon-path");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " load "

[icons]
icons = "missing\nFORGED-REPORT-LINE"
"#,
    );

    let output = doctor(&config, true);
    let stdout = stdout(&output);
    assert!(!output.status.success(), "{stdout:?}");
    assert!(stdout.contains("not found"), "{stdout:?}");
    assert!(
        !stdout.contains("\nFORGED-REPORT-LINE"),
        "a newline embedded in an icon-set path must be rendered visibly, not forge a report line:\n{stdout:?}"
    );
}

#[test]
fn reported_bar_font_escapes_control_characters() {
    let fixture = FixtureDir::new("control-font-name");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " load "
"#,
    );

    let output = doctor_with_font(&config, true, "monospace\u{1b}[31m");
    let stdout = stdout(&output);
    assert!(
        !stdout.contains('\u{1b}'),
        "a configured or auto-detected bar font must not inject terminal controls:\n{stdout:?}"
    );
}

#[cfg(unix)]
#[test]
fn reported_fontconfig_family_escapes_control_characters() {
    let fixture = FixtureDir::new("control-fontconfig-family");
    fixture.write_executable("fc-list", "#!/bin/sh\nprintf 'Fake Font\\n'\n");
    fixture.write_executable("fc-match", "#!/bin/sh\nprintf 'Fake Font\\033[31m'\n");
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "load"
format = " load "
"#,
    );

    let mut command = Command::new(env!("CARGO_BIN_EXE_i3status-rs"));
    command
        .arg("--doctor")
        .arg("--doctor-skip-live")
        .arg("--font")
        .arg("monospace")
        .arg(&config)
        .env_remove("I3RS_DBUS_NAME")
        .env("PATH", &fixture.0);
    let output = command.output().expect("run doctor with fake fontconfig");
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout:?}");
    assert!(stdout.contains("Fake Font"), "{stdout:?}");
    assert!(
        !stdout.contains('\u{1b}'),
        "font family names returned by fontconfig must not inject terminal controls:\n{stdout:?}"
    );
    let header = stdout
        .lines()
        .find(|line| line.starts_with("Name  Glyph"))
        .expect("icon-table header");
    let row = stdout
        .lines()
        .find(|line| line.starts_with("cogs"))
        .expect("cogs icon row");
    assert_eq!(
        header.find("Used by"),
        row.rfind("load"),
        "escaping a provider name must not break display-column alignment:\n{stdout:?}"
    );
}

#[cfg(unix)]
#[test]
fn reported_text_fallback_family_escapes_control_characters() {
    let fixture = FixtureDir::new("control-text-fallback-family");
    fixture.write_executable("fc-list", "#!/bin/sh\nprintf 'Fake Font\\n'\n");
    fixture.write_executable(
        "fc-match",
        "#!/bin/sh\ncase \"$*\" in\n  *charset=*) printf 'Fallback Font\\033[31m' ;;\n  *) printf 'Base Font' ;;\nesac\n",
    );
    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "custom"
command = "printf '!'"
interval = "once"
"#,
    );

    let mut command = Command::new(env!("CARGO_BIN_EXE_i3status-rs"));
    command
        .arg("--doctor")
        .arg("--font")
        .arg("monospace")
        .arg(&config)
        .env_remove("I3RS_DBUS_NAME")
        .env("PATH", path_with_front(&fixture.0));
    let output = command
        .output()
        .expect("run doctor with fake text fallback");
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout:?}");
    assert!(stdout.contains("Fallback Font"), "{stdout:?}");
    assert!(
        !stdout.contains('\u{1b}'),
        "text fallback families returned by fontconfig must not inject terminal controls:\n{stdout:?}"
    );
}

#[cfg(unix)]
#[test]
fn generated_but_unrendered_country_flag_is_not_reported_as_used() {
    let fixture = FixtureDir::new("unrendered-country-flag");
    fixture.write_executable(
        "nordvpn",
        "#!/bin/sh\nprintf '%s\\n' 'Status: Connected' 'Country: United States' \
         'Hostname: us123.nordvpn.com'\n",
    );

    let config = fixture.write(
        "config.toml",
        r#"
error_format = " $short_error_message "
error_fullscreen_format = " $full_error_message "

[[block]]
block = "vpn"
driver = "nordvpn"
format_connected = " $country "
"#,
    );

    let mut command = Command::new(env!("CARGO_BIN_EXE_i3status-rs"));
    command
        .arg("--doctor")
        .arg("--font")
        .arg("monospace")
        .arg(&config)
        .env_remove("I3RS_DBUS_NAME")
        .env("PATH", path_with_front(&fixture.0));
    let output = command.output().expect("run doctor with fake nordvpn");
    let stdout = stdout(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("United States"), "{stdout}");
    assert!(
        !stdout.contains("(US country flag)"),
        "a flag that was computed but excluded by the format was not used:\n{stdout}"
    );
    assert!(
        !stdout.contains("Country flags are drawn differently"),
        "the flag footnote must only appear when a rendered flag is reported:\n{stdout}"
    );
}
