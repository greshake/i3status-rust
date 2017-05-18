use std::cell::{RefCell, Cell};
use std::time::{Duration, Instant};
use std::sync::mpsc::Sender;
use std::thread;

use scheduler::UpdateRequest;
use input::I3barEvent;
use block::Block;
use widgets::rotatingtext::RotatingTextWidget;
use widgets::button::ButtonWidget;
use widget::{State, I3BarComponent, I3BarWidget};

use blocks::dbus::{Connection, BusType, stdintf, ConnectionItem, Message};
use self::stdintf::OrgFreedesktopDBusProperties;
use serde_json::Value;
use uuid::Uuid;

pub struct Music {
    name: String,
    current_song: RefCell<RotatingTextWidget>,
    prev: Option<RefCell<ButtonWidget>>,
    play: Option<RefCell<ButtonWidget>>,
    next: Option<RefCell<ButtonWidget>>,
    dbus_conn: Connection,
    player_avail: Cell<bool>,
    player: String,
}

impl Music {
    pub fn new(config: Value, send: Sender<UpdateRequest>, theme: &Value) -> Music {
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
            name: name_copy,
            current_song: RefCell::new(RotatingTextWidget::new(Duration::new(10, 0),
                                                               Duration::new(0, 500000000),
                                                               get_u64_default!(config, "max-width", 21) as usize,
                                                               theme.clone()).with_icon("music").with_state(State::Info)),
            prev: if let Some(p) = prev {Some(RefCell::new(p))} else {None},
            play: if let Some(p) = play {Some(RefCell::new(p))} else {None},
            next: if let Some(p) = next {Some(RefCell::new(p))} else {None},
            dbus_conn: Connection::get_private(BusType::Session).unwrap(),
            player_avail: Cell::new(false),
            player: get_str!(config, "player"),
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
                (*self.current_song.borrow_mut()).set_text(String::from(""));
                self.player_avail.set(false);
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

                (*self.current_song.borrow_mut()).set_text(format!("{} | {}", title, artist));
                self.player_avail.set(true);
            }
            if let Some(ref play) = self.play {
                let data = c.get("org.mpris.MediaPlayer2.Player", "PlaybackStatus");
                if data.is_err() {
                    (*play.borrow_mut()).set_icon("music_play")
                } else {
                    let state = data.unwrap().0;
                    if state.as_str().unwrap() != "Playing" {
                        (*play.borrow_mut()).set_icon("music_play");
                    } else {
                        (*play.borrow_mut()).set_icon("music_pause");
                    }
                }
            }
        }
        next
    }


    fn click(&self, event: &I3barEvent) {
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
                self.dbus_conn.send(m);
            }
        }
    }

    fn get_ui(&self) -> &impl I3BarComponent {
        if self.player_avail.get() {
            let mut elements: Vec<Box<I3BarWidget>> = Vec::new();
            elements.push(Box::new(I3BarWidget::WidgetWithSeparator(Box::new(self.current_song.clone().into_inner()) as Box<I3BarWidget>)));
            if let Some(ref prev) = self.prev {
                elements.push(Box::new(I3BarWidget::Widget(Box::new(prev.clone().into_inner()) as Box<I3BarWidget>)));
            }
            if let Some(ref play) = self.play {
                elements.push(Box::new(I3BarWidget::Widget(Box::new(play.clone().into_inner()) as Box<I3BarWidget>)));
            }
            if let Some(ref next) = self.next {
                elements.push(Box::new(I3BarWidget::Widget(Box::new(next.clone().into_inner()) as Box<I3BarWidget>)));
            }
            Box::new(I3BarWidget::Block(elements))
        } else {
            ui!(self.current_song)
        }
    }
}