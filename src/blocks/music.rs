use std::time::{Duration, Instant};
use chan::Sender;
use std::thread;
use std::boxed::Box;

use config::Config;
use errors::*;
use scheduler::Task;
use input::I3BarEvent;
use block::{Block, ConfigBlock};
use de::deserialize_duration;
use widgets::rotatingtext::RotatingTextWidget;
use widgets::button::ButtonWidget;
use widget::{I3BarWidget, State};

use blocks::dbus::{arg, stdintf, BusType, Connection, ConnectionItem, Message};
use blocks::dbus::arg::RefArg;
use self::stdintf::org_freedesktop_dbus::Properties;
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

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct MusicConfig {
    /// Name of the music player.Must be the same name the player<br/> is registered with the MediaPlayer2 Interface.
    pub player: String,

    /// Max width of the block in characters, not including the buttons
    #[serde(default = "MusicConfig::default_max_width")]
    pub max_width: usize,

    /// Bool to specify if a marquee style rotation should be used<br/> if the title + artist is longer than max-width
    #[serde(default = "MusicConfig::default_marquee")]
    pub marquee: bool,

    /// Marquee interval in seconds. This is the delay between each rotation.
    #[serde(default = "MusicConfig::default_marquee_interval", deserialize_with = "deserialize_duration")]
    pub marquee_interval: Duration,

    /// Marquee speed in seconds. This is the scrolling time used per character.
    #[serde(default = "MusicConfig::default_marquee_speed", deserialize_with = "deserialize_duration")]
    pub marquee_speed: Duration,

    /// Array of control buttons to be displayed. Options are<br/>prev (previous title), play (play/pause) and next (next title)
    #[serde(default = "MusicConfig::default_buttons")]
    pub buttons: Vec<String>,
}

impl MusicConfig {
    fn default_max_width() -> usize {
        21
    }

    fn default_marquee() -> bool {
        true
    }

    fn default_marquee_interval() -> Duration {
        Duration::from_secs(10)
    }

    fn default_marquee_speed() -> Duration {
        Duration::from_millis(500)
    }

    fn default_buttons() -> Vec<String> {
        vec![]
    }
}

impl ConfigBlock for Music {
    type Config = MusicConfig;

    fn new(block_config: Self::Config, config: Config, send: Sender<Task>) -> Result<Self> {
        let id: String = format!("{}", Uuid::new_v4().to_simple());
        let id_copy = id.clone();

        thread::spawn(move || {
            let c = Connection::get_private(BusType::Session).unwrap();
            c.add_match(
                "interface='org.freedesktop.DBus.Properties',member='PropertiesChanged'",
            ).unwrap();
            loop {
                for ci in c.iter(100000) {
                    if let ConnectionItem::Signal(msg) = ci {
                        if &*msg.path().unwrap() == "/org/mpris/MediaPlayer2" && &*msg.member().unwrap() == "PropertiesChanged" {
                            send.send(Task {
                                id: id.clone(),
                                update_time: Instant::now(),
                            });
                        }
                    }
                }
            }
        });

        let mut play: Option<ButtonWidget> = None;
        let mut prev: Option<ButtonWidget> = None;
        let mut next: Option<ButtonWidget> = None;
        for button in block_config.buttons {
            match &*button {
                "play" => {
                    play = Some(
                        ButtonWidget::new(config.clone(), "play")
                            .with_icon("music_play")
                            .with_state(State::Info),
                    )
                }
                "next" => {
                    next = Some(
                        ButtonWidget::new(config.clone(), "next")
                            .with_icon("music_next")
                            .with_state(State::Info),
                    )
                }
                "prev" => {
                    prev = Some(
                        ButtonWidget::new(config.clone(), "prev")
                            .with_icon("music_prev")
                            .with_state(State::Info),
                    )
                }
                x => Err(BlockError(
                    "music".to_owned(),
                    format!("unknown music button identifier: '{}'", x),
                ))?,
            };
        }

        Ok(Music {
            id: id_copy,
            current_song: RotatingTextWidget::new(
                Duration::new(block_config.marquee_interval.as_secs(), 0),
                Duration::new(0, block_config.marquee_speed.subsec_nanos()),
                block_config.max_width,
                config.clone(),
            ).with_icon("music")
                .with_state(State::Info),
            prev: prev,
            play: play,
            next: next,
            dbus_conn: Connection::get_private(BusType::Session)
                .block_error("music", "failed to establish D-Bus connection")?,
            player_avail: false,
            player: block_config.player,
            marquee: block_config.marquee,
        })
    }
}

impl Block for Music {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Duration>> {
        let (rotated, next) = if self.marquee {
            self.current_song.next()?
        } else {
            (false, None)
        };

        if !rotated {
            let c = self.dbus_conn.with_path(
                format!("org.mpris.MediaPlayer2.{}", self.player),
                "/org/mpris/MediaPlayer2",
                1000,
            );
            let data = c.get("org.mpris.MediaPlayer2.Player", "Metadata");

            if data.is_err() {
                self.current_song.set_text(String::from(""));
                self.player_avail = false;
            } else {
                let metadata = data.unwrap();

                let (title, artist) = extract_from_metadata(&metadata).unwrap_or((String::new(), String::new()));

                if title.is_empty() && artist.is_empty() {
                    self.player_avail = false;
                    self.current_song.set_text(String::new());
                } else {
                    self.player_avail = true;
                    self.current_song
                        .set_text(format!("{} | {}", title, artist));
                }
            }
            if let Some(ref mut play) = self.play {
                let data = c.get("org.mpris.MediaPlayer2.Player", "PlaybackStatus");
                match data {
                    Err(_) => play.set_icon("music_play"),
                    Ok(data) => {
                        let data: Box<RefArg> = data;
                        let state = data;
                        if state.as_str().map(|s| s != "Playing").unwrap_or(false) {
                            play.set_icon("music_play")
                        } else {
                            play.set_icon("music_pause")
                        }
                    }
                }
            }
        }
        Ok(match (next, self.marquee) {
            (Some(_), _) => next,
            (None, true) => Some(Duration::new(1, 0)),
            (None, false) => Some(Duration::new(1, 0)),
        })
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = event.name {
            let action = match name as &str {
                "play" => "PlayPause",
                "next" => "Next",
                "prev" => "Previous",
                _ => "",
            };
            if action != "" {
                let m = Message::new_method_call(
                    format!("org.mpris.MediaPlayer2.{}", self.player),
                    "/org/mpris/MediaPlayer2",
                    "org.mpris.MediaPlayer2.Player",
                    action,
                ).block_error("music", "failed to create D-Bus method call")?;
                self.dbus_conn
                    .send(m)
                    .block_error("music", "failed to call method via D-Bus")
                    .map(|_| ())
            } else {
                Ok(())
            }
        } else {
            Ok(())
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
            vec![&self.current_song]
        }
    }
}

fn extract_from_metadata(metadata: &Box<arg::RefArg>) -> Result<(String, String)> {
    let mut title = String::new();
    let mut artist = String::new();

    let mut iter = metadata
        .as_iter()
        .block_error("music", "failed to extract metadata")?;

    while let Some(key) = iter.next() {
        let value = iter.next()
            .block_error("music", "failed to extract metadata")?;
        match key.as_str()
            .block_error("music", "failed to extract metadata")?
        {
            "xesam:artist" => {
                artist = String::from(value
                    .as_iter()
                    .block_error("music", "failed to extract metadata")?
                    .nth(0)
                    .block_error("music", "failed to extract metadata")?
                    .as_iter()
                    .block_error("music", "failed to extract metadata")?
                    .nth(0)
                    .block_error("music", "failed to extract metadata")?
                    .as_iter()
                    .block_error("music", "failed to extract metadata")?
                    .nth(0)
                    .block_error("music", "failed to extract metadata")?
                    .as_str()
                    .block_error("music", "failed to extract metadata")?)
            }
            "xesam:title" => {
                title = String::from(value
                    .as_str()
                    .block_error("music", "failed to extract metadata")?)
            }
            _ => {}
        };
    }
    Ok((title, artist))
}
