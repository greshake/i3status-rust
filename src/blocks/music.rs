use std::cell::{RefCell};
use std::time::{Duration, Instant};
use std::sync::mpsc::Sender;
use std::thread;

use scheduler::UpdateRequest;
use block::{Block, State};
use blocks::rotatingtext::RotatingText;

use blocks::dbus::{Connection, BusType, stdintf, ConnectionItem};
use self::stdintf::OrgFreedesktopDBusProperties;
use serde_json::Value;
use uuid::Uuid;

pub struct Music {
    name: String,
    current_song: RefCell<RotatingText>,
    dbus_conn: Connection,
    player: String,
}

impl Music {
    pub fn new(config: Value, send: Sender<UpdateRequest>) -> Music {
        let name: String = Uuid::new_v4().simple().to_string();
        let name_copy = name.clone();

        thread::spawn(move || {
            let c = Connection::get_private(BusType::Session).unwrap();
            c.add_match("interface='org.freedesktop.DBus.Properties',member='PropertiesChanged'").unwrap();
            loop {
                for ci in c.iter(100000) {
                    match ci {
                        ConnectionItem::Signal(msg) => {
                            if &*msg.path().unwrap() == "/org/mpris/MediaPlayer2" {
                                if &*msg.member().unwrap() == "PropertiesChanged" {
                                    send.send(UpdateRequest {
                                        id: name.clone(),
                                        update_time: Instant::now()
                                    });
                                }
                            }
                        }, _ => {}
                    }
                }
            }
        });

        Music {
            name: name_copy,
            current_song: RefCell::new(RotatingText::new(Duration::new(10, 0),
                                                         Duration::new(0, 500000000),
                                                         get_u64_default!(config, "max-width", 20) as usize)),
            dbus_conn: Connection::get_private(BusType::Session).unwrap(),
            player: get_str!(config, "player")
        }
    }
}


impl Block for Music
{
    fn id(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn update(&self) -> Option<Duration> {
        let (rotated, next) = (*self.current_song.borrow_mut()).next();

        if !rotated {
            let c = self.dbus_conn.with_path(
            format!("org.mpris.MediaPlayer2.{}", self.player),
            "/org/mpris/MediaPlayer2", 1000);
            let data = c.get("org.mpris.MediaPlayer2.Player", "Metadata");

            if data.is_err() {
                (*self.current_song.borrow_mut()).set_content(String::from(" "));
            } else {
                let metadata = data.unwrap();

                let mut title = String::new();
                let mut artist = String::new();

                let mut iter = metadata.0.as_iter().unwrap();

                while let Some(key) = iter.next() {
                    let value = iter.next().unwrap();
                    match key.as_str().unwrap() {
                        "xesam:artist" => {
                            artist = String::from(value.as_iter().unwrap().nth(0).unwrap()
                                .as_iter().unwrap().nth(0).unwrap()
                                .as_iter().unwrap().nth(0).unwrap()
                                .as_str().unwrap())
                        },
                        "xesam:title" => {
                            title = String::from(value.as_str().unwrap())
                        }
                        _ => {}
                    };
                }

                (*self.current_song.borrow_mut()).set_content(format!(" {} | {} ", title, artist));
            }
        }
        next
    }

    fn get_status(&self, theme: &Value) -> Value {
        json!({
            "full_text" : format!("{}{}", theme["icons"]["music"].as_str().unwrap(),
                                            self.current_song.borrow().to_string()),
            "min_width": if self.current_song.borrow().to_string() == " " {0} else {240},
            "align": "left"
        })
    }

    fn get_state(&self) -> State {
        State::Info
    }
}