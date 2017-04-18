use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};
use std::sync::mpsc::Sender;
use std::thread;

use block::{Block, State};
use input::I3barEvent;
use scheduler::UpdateRequest;

use blocks::dbus::{Connection, Message, BusType, stdintf, ConnectionItem};
use self::stdintf::OrgFreedesktopDBusProperties;
use serde_json::Value;
use uuid::Uuid;


pub struct MusicPlayButton {
    name: String,
    playing: Cell<bool>,
    player: String,
    dbus_conn: Connection
}

impl MusicPlayButton {
    pub fn new(config: Value, tx: Sender<UpdateRequest>) -> MusicPlayButton {
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
                                    tx.send(UpdateRequest {
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

        MusicPlayButton {
            name: name_copy,
            playing: Cell::new(false),
            dbus_conn: Connection::get_private(BusType::Session).unwrap(),
            player: get_str!(config, "player"),
        }
    }
}


impl Block for MusicPlayButton
{
    fn id(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn update(&self) -> Option<Duration> {
        let c = self.dbus_conn.with_path(
            format!("org.mpris.MediaPlayer2.{}", self.player),
            "/org/mpris/MediaPlayer2", 1000);
        let data = c.get("org.mpris.MediaPlayer2.Player", "PlaybackStatus");
        if data.is_err() {
            self.playing.set(false);
        } else {
            let state = data.unwrap().0;
            if state.as_str().unwrap() != "Playing" {
                self.playing.set(false);
            } else {
                self.playing.set(true);
            }
        }
        None
    }

    fn get_status(&self, theme: &Value) -> Value {
        json!({
            "full_text" : if self.playing.get() {theme["icons"]["music_pause"].as_str().unwrap()}
                            else {theme["icons"]["music_play"].as_str().unwrap()}
        })
    }

    fn get_state(&self) -> State {
        State::Idle
    }

    fn click(&self, event: I3barEvent) {
        let m = Message::new_method_call(format!("org.mpris.MediaPlayer2.{}",
                                                 self.player),
                                         "/org/mpris/MediaPlayer2",
                                         "org.mpris.MediaPlayer2.Player",
                                         "PlayPause").unwrap();
        self.dbus_conn.send(m);
    }
}