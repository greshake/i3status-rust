use std::time::Duration;
use std::process::Command;
use std::env;

use block::Block;
use widgets::text::TextWidget;
use widget::I3BarWidget;

use serde_json::Value;
use uuid::Uuid;


pub struct Pacman {
    text: TextWidget,
    id: String,
    update_interval: Duration,
}

impl Pacman {
    pub fn new(config: Value, theme: Value) -> Pacman {
        {
            Pacman {
                id: Uuid::new_v4().simple().to_string(),
                update_interval: Duration::new(get_u64_default!(config, "interval", 600), 0),
                text: TextWidget::new(theme.clone()).with_icon("update"),
            }
        }
    }
}

fn run_command(var: &str) {
    Command::new("sh")
        .args(&["-c", var])
        .spawn();
}
        

fn get_update_count() -> usize {
    let tmp_dir = env::temp_dir().into_os_string().into_string()
        .expect("There's something wrong with your $TMP variable");
    let user = match env::var_os("USER") {
        Some(var) => var.into_string()
            .expect("There's something wrong with your $USER"),
        None => String::from("")
    };
    let updates_db = match env::var_os("CHECKUPDATES_DB") {
        Some(var) => var.into_string()
            .expect("There's a problem with your $CHECKUPDATES_DB"),
        None => format!("{}/checkup-db-{}", tmp_dir, user)
    };
    
    run_command(&format!("trap 'rm -f {}/db.lck' INT TERM EXIT", updates_db));
    let db_path = match env::var_os("DBPath") {
        Some(var) => var.into_string()
            .expect("There's something wrong with your $DBPath"),
        None => String::from("/var/lib/pacman/")
    };
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
        self.text.set_text(format!("{}", count));
        Some(self.update_interval.clone())
    }
    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }
    fn id(&self) -> &str {
        &self.id
    }
}
