use std::time::Duration;
use std::process::Command;
use std::env;
use std::ffi::OsString;

use block::Block;
use input::I3barEvent;
use widgets::text::TextWidget;
use widget::{I3BarWidget, State};

use serde_json::Value;
use uuid::Uuid;


pub struct Pacman {
    output: TextWidget,
    id: String,
    update_interval: Duration,
}

impl Pacman {
    pub fn new(config: Value, theme: Value) -> Pacman {
        {
            Pacman {
                id: Uuid::new_v4().simple().to_string(),
                update_interval: Duration::new(get_u64_default!(config, "interval", 600), 0),
                output: TextWidget::new(theme.clone()).with_icon("update"),
            }
        }
    }
}

fn run_command(var: &str) {
    Command::new("sh")
        .args(&["-c", var])
        .spawn();
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

    run_command(&format!("trap 'rm -f {}/db.lck' INT TERM EXIT", updates_db));
    let db_path = env::var_os("DBPath").unwrap_or(OsString::from("/var/lib/pacman/"))
        .into_string().expect("There's a problem with your $DBPath");
    run_command("awk -F' *= *' '$1 ~ /DBPATH/ { print $1 \"=\" 2 }' /etc/pacman.conf");
    run_command(&format!("mkdir -p \"{}\"", updates_db));
    run_command(&format!("ln -s \"{}/local\" \"{}\" &> /dev/null", db_path, updates_db));
    run_command(&format!("fakeroot -- pacman -Sy --dbpath \"{}\" --logfile /dev/null &> /dev/null", updates_db));
    String::from_utf8(
    Command::new("sh")
        .args(&["-c", &format!("fakeroot pacman -Su -p --dbpath \"{}\"", updates_db)])
        .output().expect("There was a problem running the pacman commands")
        .stdout).expect("there was a problem parsing the output")
        .lines().count() - 1
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

    fn click(&mut self, event: &I3barEvent) {
        if event.name
            .as_ref()
            .map(|s| s == "pacman")
            .unwrap_or(false) && event.button == 1 /* left mouse button */ {
            self.update();
        }
    }
}
