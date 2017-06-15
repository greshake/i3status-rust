use std::fs;
use std::path::Path;
use std::os::unix::fs::symlink;
use std::time::Duration;
use std::process::Command;
use std::env;
use std::ffi::OsString;

use block::Block;
use config::Config;
use input::{I3BarEvent, MouseButton};
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};

use toml::value::Value;
use uuid::Uuid;


pub struct Pacman {
    output: TextWidget,
    id: String,
    update_interval: Duration,
}

impl Pacman {
    pub fn new(block_config: Value, config: Config) -> Pacman {
        Pacman {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: Duration::new(get_u64_default!(block_config, "interval", 600), 0),
            output: TextWidget::new(config).with_icon("update"),
        }
    }
}

fn run_command(var: &str) {
    Command::new("sh")
        .args(&["-c", var])
        .spawn()
        .expect(&format!("Failed to run command '{}'", var))
        .wait()
        .expect(&format!("Failed to wait for command '{}'", var));
}

fn has_fake_root() -> bool {
    match String::from_utf8(
        Command::new("sh")
            .args(&["-c", "type -P fakeroot"])
            .output().unwrap().stdout).unwrap().trim() {
        "" => return false,
        _ => return true,
    }
}


fn get_update_count() -> usize {
    if !has_fake_root() {
        return 0 as usize
    }
    let tmp_dir = env::temp_dir().into_os_string().into_string()
        .expect("There's something wrong with your $TMP variable");
    let user = env::var_os("USER").unwrap_or(OsString::from("")).into_string()
        .expect("There's a problem with your $USER");
    let updates_db = env::var_os("CHECKUPDATES_DB")
        .unwrap_or(OsString::from(format!("{}/checkup-db-{}", tmp_dir, user)))
        .into_string().expect("There's a problem with your $CHECKUPDATES_DB");

    // Determine pacman database path
    let db_path = env::var_os("DBPath")
        .map(Into::into)
        .unwrap_or(Path::new("/var/lib/pacman/").to_path_buf());

    // Create the determined `checkup-db` path recursively
    fs::create_dir_all(&updates_db)
        .expect(&format!("Failed to create checkup-db path '{}'", updates_db));

    // Create symlink to local cache in `checkup-db` if required
    let local_cache = Path::new(&updates_db).join("local");
    if !local_cache.exists() {
        symlink(db_path.join("local"), local_cache)
            .expect("Failed to created required symlink");
    }

    // Update database
    run_command(&format!("fakeroot -- pacman -Sy --dbpath \"{}\" --logfile /dev/null &> /dev/null", updates_db));

    // Get update count
    String::from_utf8(
        Command::new("sh")
            .args(&["-c", &format!("fakeroot pacman -Su -p --dbpath \"{}\"", updates_db)])
            .output().expect("There was a problem running the pacman commands")
            .stdout)
        .expect("there was a problem parsing the output")
        .lines()
        .count() - 1
}


impl Block for Pacman
{
    fn update(&mut self) -> Option<Duration> {
        let count = get_update_count();
        self.output.set_text(format!("{}", count));
        self.output.set_state(match count {
            0 => State::Idle,
            _ => State::Info
        });
        Some(self.update_interval.clone())
    }
    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }
    fn id(&self) -> &str {
        &self.id
    }

    fn click(&mut self, event: &I3BarEvent) {
        if event.name
            .as_ref()
            .map(|s| s == "pacman")
            .unwrap_or(false) && event.button == MouseButton::Left {
            self.update();
        }
    }
}
