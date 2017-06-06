use std::time::{Duration, Instant};
use std::sync::mpsc::Sender;
use std::thread;
use std::boxed::Box;

use scheduler::Task;
use input::I3barEvent;
use block::Block;
use widgets::rotatingtext::RotatingTextWidget;
use widgets::button::ButtonWidget;
use widget::{State, I3BarWidget};

use blocks::dbus::{Connection, BusType, stdintf, ConnectionItem, Message, arg};
use self::stdintf::OrgFreedesktopDBusProperties;
use serde_json::Value;
use uuid::Uuid;

pub struct Music {
    id: String,
    current_song: RotatingTextWidget,
    prev: Option<ButtonWidget>,
    play: Option<ButtonWidget>,
    next: Option<ButtonWidget>,
    dbus_conn: Connection,
    player_avail: bool,
    marquee: bool,
    player: String,
}

impl Music {
    pub fn new(config: Value, send: Sender<Task>, theme: &Value) -> Music {
        let id: String = Uuid::new_v4().simple().to_string();
        let id_copy = id.clone();

        thread::spawn(move || {
            let c = Connection::get_private(BusType::Session).unwrap();
            c.add_match("interface='org.freedesktop.DBus.Properties',member='PropertiesChanged'").unwrap();
            loop {
                for ci in c.iter(100000) {
                    match ci {
                        ConnectionItem::Signal(msg) => {
                            if &*msg.path().unwrap() == "/org/mpris/MediaPlayer2" {
                                if &*msg.member().unwrap() == "PropertiesChanged" {
                                    send.send(Task {
                                        id: id.clone(),
                                        update_time: Instant::now()
                                    }).unwrap();
                                }
                            }
                        }, _ => {}
                    }
                }
            }
        });

        let buttons = config["buttons"].as_array().expect("'buttons' must be an array of 'play', 'next' and/or 'prev'!");
        let mut play: Option<ButtonWidget> = None;
        let mut prev: Option<ButtonWidget> = None;
        let mut next: Option<ButtonWidget> = None;
        for button in buttons {
            match button.as_str().expect("Music button identifiers must be Strings") {
                "play" =>
                    play = Some(ButtonWidget::new(theme.clone(), "play")
                        .with_icon("music_play").with_state(State::Info)),
                "next" =>
                    next = Some(ButtonWidget::new(theme.clone(), "next")
                        .with_icon("music_next").with_state(State::Info)),
                "prev" =>
                    prev = Some(ButtonWidget::new(theme.clone(), "prev")
                        .with_icon("music_prev").with_state(State::Info)),
                x => panic!("Unknown Music button identifier! {}", x)
            };
        }

        Music {
            id: id_copy,
            current_song: RotatingTextWidget::new(Duration::new(10, 0),
                                                               Duration::new(0, 500000000),
                                                               get_u64_default!(config, "max_width", 21) as usize,
                                                               theme.clone()).with_icon("music").with_state(State::Info),
            prev: prev,
            play: play,
            next: next,
            dbus_conn: Connection::get_private(BusType::Session).unwrap(),
            player_avail: false,
            player: get_str!(config, "player"),
            marquee: get_bool_default!(config, "marquee", true),
        }
    }
}


impl Block for Music
{
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Option<Duration> {
        let (rotated, next) = if self.marquee {self.current_song.next()} else {(false, None)};

        if !rotated {
            let c = self.dbus_conn.with_path(
            format!("org.mpris.MediaPlayer2.{}", self.player),
            "/org/mpris/MediaPlayer2", 1000);
            let data = c.get("org.mpris.MediaPlayer2.Player", "Metadata");

            if data.is_err() {
                self.current_song.set_text(String::from(""));
                self.player_avail = false;
            } else {
                let metadata = data.unwrap();

                let (title, artist) = extract_from_metadata(metadata).unwrap_or((String::new(), String::new()));

                self.current_song.set_text(format!("{} | {}", title, artist));
                self.player_avail = true;
            }
            if let Some(ref mut play) = self.play {
                let data = c.get("org.mpris.MediaPlayer2.Player", "PlaybackStatus");
                if data.is_err() {
                    play.set_icon("music_play")
                } else {
                    let state = data.unwrap().0;
                    if state.as_str().unwrap() != "Playing" {
                        play.set_icon("music_play");
                    } else {
                        play.set_icon("music_pause");
                    }
                }
            }
        }
        if next.is_none() && !self.marquee {
            None
        } else if next.is_none() {
            Some(Duration::new(1, 0))
        } else {
            next
        }
    }


    fn click_left(&mut self, event: &I3barEvent) {
        if event.name.is_some() {
            let action = match &event.name.clone().unwrap() as &str {
                "play" => "PlayPause",
                "next" => "Next",
                "prev" => "Previous",
                _ => ""
            };
            if action != "" {
                let m = Message::new_method_call(format!("org.mpris.MediaPlayer2.{}",
                                                         self.player),
                                                 "/org/mpris/MediaPlayer2",
                                                 "org.mpris.MediaPlayer2.Player",
                                                 action).unwrap();
                self.dbus_conn.send(m).unwrap();
            }
        }
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        if self.player_avail {
            let mut elements: Vec<&I3BarWidget> = Vec::new();
            elements.push(&self.current_song);
            if let Some(ref prev) = self.prev {
                elements.push(prev);
            }
            if let Some(ref play) = self.play {
                elements.push(play);;
            }
            if let Some(ref next) = self.next {
                elements.push(next);;
            }
            elements
        } else {
            vec!(&self.current_song)
        }
    }
}

fn extract_from_metadata(metadata: arg::Variant<Box<arg::RefArg>>) -> Result<(String, String), ()> {
    let mut title = String::new();
    let mut artist = String::new();

    let mut iter = metadata.0.as_iter().ok_or(())?;

    while let Some(key) = iter.next() {
        let value = iter.next().ok_or(())?;
        match key.as_str().ok_or(())? {
            "xesam:artist" => {
                artist = String::from(value.as_iter().ok_or(())?.nth(0).ok_or(())?
                    .as_iter().ok_or(())?.nth(0).ok_or(())?
                    .as_iter().ok_or(())?.nth(0).ok_or(())?
                    .as_str().ok_or(())?)
            },
            "xesam:title" => {
                title = String::from(value.as_str().ok_or(())?)
            }
            _ => {}
        };
    }
    Ok((title, artist))
}
