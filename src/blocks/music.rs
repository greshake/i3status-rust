use std::boxed::Box;
use std::result;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::arg::{Array, RefArg};
use dbus::ffidisp::stdintf::org_freedesktop_dbus::Properties;
use dbus::{
    arg,
    ffidisp::{BusType, Connection, ConnectionItem},
    Message,
};
use regex::Regex;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::Update;
use crate::blocks::{Block, ConfigBlock};
use crate::config::{Config, LogicalDirection};
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;
use crate::widgets::rotatingtext::RotatingTextWidget;

pub struct Music {
    id: String,
    current_song: RotatingTextWidget,
    prev: Option<ButtonWidget>,
    play: Option<ButtonWidget>,
    next: Option<ButtonWidget>,
    on_collapsed_click_widget: ButtonWidget,
    on_collapsed_click: Option<String>,
    on_click: Option<String>,
    dbus_conn: Connection,
    player_avail: bool,
    marquee: bool,
    player: Option<String>,
    auto_discover: bool,
    smart_trim: bool,
    max_width: usize,
    separator: String,
    seek_step: i64,
    config: Config,
    interface_name_exclude_regexps: Vec<Regex>,
    hide_when_empty: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct MusicConfig {
    /// Name of the music player.Must be the same name the player<br/> is registered with the MediaPlayer2 Interface.
    /// Set an empty string for auto-discovery of currently active player.
    pub player: Option<String>,

    /// Max width of the block in characters, not including the buttons
    #[serde(default = "MusicConfig::default_max_width")]
    pub max_width: usize,

    /// Bool to specify whether the block will change width depending on the text content
    /// or remain static always (= max_width)
    #[serde(default = "MusicConfig::default_dynamic_width")]
    pub dynamic_width: bool,

    /// Bool to specify if a marquee style rotation should be used<br/> if the title + artist is longer than max-width
    #[serde(default = "MusicConfig::default_marquee")]
    pub marquee: bool,

    /// Marquee interval in seconds. This is the delay between each rotation.
    #[serde(
        default = "MusicConfig::default_marquee_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub marquee_interval: Duration,

    /// Marquee speed in seconds. This is the scrolling time used per character.
    #[serde(
        default = "MusicConfig::default_marquee_speed",
        deserialize_with = "deserialize_duration"
    )]
    pub marquee_speed: Duration,

    /// Bool to specify whether smart trimming should be used when marquee rotation is disabled<br/> and the title + artist is longer than max-width. It will trim from both the artist and the title in proportion to their lengths, to try and show the most information possible.
    #[serde(default = "MusicConfig::default_smart_trim")]
    pub smart_trim: bool,

    /// Separator to use between artist and title.
    #[serde(default = "MusicConfig::default_separator")]
    pub separator: String,

    /// Array of control buttons to be displayed. Options are<br/>prev (previous title), play (play/pause) and next (next title)
    #[serde(default = "MusicConfig::default_buttons")]
    pub buttons: Vec<String>,

    #[serde(default = "MusicConfig::default_on_collapsed_click")]
    pub on_collapsed_click: Option<String>,

    #[serde(default = "MusicConfig::default_on_click")]
    pub on_click: Option<String>,

    // Number of microseconds to seek forward/backward when scrolling on the bar.
    #[serde(default = "MusicConfig::default_seek_step")]
    pub seek_step: i64,

    /// MPRIS interface name regex patterns to ignore.
    #[serde(default = "MusicConfig::default_interface_name_exclude_patterns")]
    pub interface_name_exclude: Vec<String>,

    #[serde(default = "MusicConfig::default_hide_when_empty")]
    pub hide_when_empty: bool,
}

impl MusicConfig {
    fn default_max_width() -> usize {
        21
    }

    fn default_dynamic_width() -> bool {
        false
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

    fn default_smart_trim() -> bool {
        false
    }

    fn default_separator() -> String {
        " - ".to_string()
    }

    fn default_buttons() -> Vec<String> {
        vec![]
    }

    fn default_on_collapsed_click() -> Option<String> {
        None
    }

    fn default_on_click() -> Option<String> {
        None
    }

    fn default_seek_step() -> i64 {
        1000
    }

    fn default_interface_name_exclude_patterns() -> Vec<String> {
        vec![]
    }

    fn default_hide_when_empty() -> bool {
        false
    }
}

impl ConfigBlock for Music {
    type Config = MusicConfig;

    fn new(block_config: Self::Config, config: Config, send: Sender<Task>) -> Result<Self> {
        let id: String = Uuid::new_v4().to_simple().to_string();
        let id_copy = id.clone();
        let id_copy2 = id.clone();

        thread::Builder::new().name("music".into()).spawn(move || {
            let c = Connection::get_private(BusType::Session).unwrap();
            c.add_match("interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',path='/org/mpris/MediaPlayer2'")
                .unwrap();
            loop {
                for ci in c.iter(100_000) {
                    if let ConnectionItem::Signal(_) = ci {
                        send.send(Task {
                            id: id.clone(),
                            update_time: Instant::now(),
                        })
                        .unwrap();
                    }
                }
            }
        }).unwrap();

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
                x => {
                    return Err(BlockError(
                        "music".to_owned(),
                        format!("unknown music button identifier: '{}'", x),
                    ))
                }
            };
        }

        fn compile_regexps(patterns: Vec<String>) -> result::Result<Vec<Regex>, regex::Error> {
            patterns.iter().map(|p| Regex::new(&p)).collect()
        }

        Ok(Music {
            id: id_copy,
            current_song: RotatingTextWidget::new(
                Duration::new(block_config.marquee_interval.as_secs(), 0),
                Duration::new(0, block_config.marquee_speed.subsec_nanos()),
                block_config.max_width,
                block_config.dynamic_width,
                config.clone(),
                &id_copy2,
            )
            .with_icon("music")
            .with_state(State::Info),
            prev,
            play,
            next,
            on_click: block_config.on_click,
            on_collapsed_click_widget: ButtonWidget::new(config.clone(), "on_collapsed_click")
                .with_icon("music")
                .with_state(State::Info),
            on_collapsed_click: block_config.on_collapsed_click,
            dbus_conn: Connection::get_private(BusType::Session)
                .block_error("music", "failed to establish D-Bus connection")?,
            player_avail: false,
            auto_discover: block_config.player.is_none(),
            player: if block_config.player.is_none() {
                block_config.player
            } else {
                Some(format!(
                    "org.mpris.MediaPlayer2.{}",
                    block_config.player.unwrap()
                ))
            },
            marquee: block_config.marquee,
            smart_trim: block_config.smart_trim,
            max_width: block_config.max_width,
            separator: block_config.separator,
            seek_step: block_config.seek_step,
            config,
            interface_name_exclude_regexps: compile_regexps(block_config.interface_name_exclude)
                .block_error("music", "failed to parse exclude patterns")?,
            hide_when_empty: block_config.hide_when_empty,
        })
    }
}

impl Block for Music {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let (rotated, next) = if self.marquee {
            self.current_song.next()?
        } else {
            (false, None)
        };
        if !rotated && self.player.is_none() {
            self.player =
                get_first_available_player(&self.dbus_conn, &self.interface_name_exclude_regexps)
        }
        if !(rotated || self.player.is_none()) {
            let c = self.dbus_conn.with_path(
                self.player.clone().unwrap(),
                "/org/mpris/MediaPlayer2",
                1000,
            );
            let data = c.get("org.mpris.MediaPlayer2.Player", "Metadata");

            if let Ok(metadata) = data {
                let (mut title, mut artist) =
                    extract_from_metadata(&metadata).unwrap_or((String::new(), String::new()));

                if title.is_empty() && artist.is_empty() {
                    self.player_avail = false;
                    self.current_song.set_text(String::new());
                } else {
                    self.player_avail = true;

                    let textlen = title.chars().count()
                        + self.separator.chars().count()
                        + artist.chars().count();
                    if textlen < self.max_width || !self.smart_trim {
                        self.current_song
                            .set_text(format!("{}{}{}", title, self.separator, artist));
                    } else if title.is_empty() {
                        // Only display artist, truncated appropriately
                        self.current_song.set_text({
                            match artist.char_indices().nth(self.max_width) {
                                None => artist.to_string(),
                                Some((i, _)) => {
                                    artist.truncate(i);
                                    artist.to_string()
                                }
                            }
                        });
                    } else if artist.is_empty() {
                        // Only display title, truncated appropriately
                        self.current_song.set_text({
                            match title.char_indices().nth(self.max_width) {
                                None => title.to_string(),
                                Some((i, _)) => {
                                    title.truncate(i);
                                    title.to_string()
                                }
                            }
                        });
                    } else {
                        // Below code is by https://github.com/jgbyrne
                        let text = format!("{}{}{}", title, self.separator, artist);
                        if textlen > self.max_width {
                            // overshoot: # of chars we need to trim
                            // substance: # of chars available for trimming
                            let overshoot = (textlen - self.max_width) as f32;
                            let substance = (textlen - 3) as f32;

                            // Calculate number of chars to trim from title
                            let tlen = title.chars().count();
                            let tblm = tlen as f32 / substance;
                            let mut tnum = (overshoot * tblm).ceil() as usize;

                            // Calculate number of chars to trim from artist
                            let alen = artist.chars().count();
                            let ablm = alen as f32 / substance;
                            let mut anum = (overshoot * ablm).ceil() as usize;

                            // Prefer to only trim one of the title and artist
                            if anum < tnum && anum <= 3 && (tnum + anum < tlen) {
                                anum = 0;
                                tnum += anum;
                            }

                            if tnum < anum && tnum <= 3 && (anum + tnum < alen) {
                                tnum = 0;
                                anum += tnum;
                            }

                            // Calculate how many chars to keep from title and artist
                            let mut ttrc = tlen - tnum;
                            if ttrc < 1 || ttrc > 5000 {
                                ttrc = 1
                            }

                            let mut atrc = alen - anum;
                            if atrc < 1 || atrc > 5000 {
                                atrc = 1
                            }

                            // Truncate artist and title to appropriate lengths
                            let tidx = title
                                .char_indices()
                                .nth(ttrc)
                                .unwrap_or((title.len(), 'a'))
                                .0;
                            title.truncate(tidx);

                            let aidx = artist
                                .char_indices()
                                .nth(atrc)
                                .unwrap_or((artist.len(), 'a'))
                                .0;
                            artist.truncate(aidx);

                            // Produce final formatted string
                            self.current_song
                                .set_text(format!("{}{}{}", title, self.separator, artist));
                        } else {
                            self.current_song.set_text(text);
                        }
                    }
                }
            } else {
                self.current_song.set_text(String::from(""));
                self.player_avail = false;
                if self.auto_discover {
                    self.player = None;
                }
            }

            if let Some(ref mut play) = self.play {
                let data = c.get("org.mpris.MediaPlayer2.Player", "PlaybackStatus");
                match data {
                    Err(_) => play.set_icon("music_play"),
                    Ok(data) => {
                        let data: Box<dyn RefArg> = data;
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
            (Some(_), _) => next.map(|d| d.into()),
            (None, _) => Some(Duration::new(2, 0).into()),
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

            match event.button {
                MouseButton::Left => {
                    if action != "" {
                        let m = Message::new_method_call(
                            self.player.as_ref().unwrap(),
                            "/org/mpris/MediaPlayer2",
                            "org.mpris.MediaPlayer2.Player",
                            action,
                        )
                        .block_error("music", "failed to create D-Bus method call")?;
                        self.dbus_conn
                            .send(m)
                            .block_error("music", "failed to call method via D-Bus")?;
                    } else {
                        if name == "on_collapsed_click" && self.on_collapsed_click.is_some() {
                            let command = self.on_collapsed_click.as_ref().unwrap();
                            spawn_child_async("sh", &["-c", command])
                                .block_error("music", "could not spawn child")?;
                        } else if event.matches_name(self.id()) {
                            if let Some(ref cmd) = self.on_click {
                                spawn_child_async("sh", &["-c", cmd])
                                    .block_error("music", "could not spawn child")?;
                            }
                        }
                    }
                }
                _ => {
                    if name.as_str() == self.id {
                        let m = Message::new_method_call(
                            self.player.as_ref().unwrap(),
                            "/org/mpris/MediaPlayer2",
                            "org.mpris.MediaPlayer2.Player",
                            "Seek",
                        )
                        .block_error("music", "failed to create D-Bus method call")?;

                        use LogicalDirection::*;
                        match self.config.scrolling.to_logical_direction(event.button) {
                            Some(Up) => {
                                self.dbus_conn
                                    .send(m.append1(self.seek_step * 1000))
                                    .block_error("music", "failed to call method via D-Bus")?;
                            }
                            Some(Down) => {
                                self.dbus_conn
                                    .send(m.append1(self.seek_step * -1000))
                                    .block_error("music", "failed to call method via D-Bus")?;
                            }
                            None => {}
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        if !self.player_avail && self.hide_when_empty {
            vec![]
        } else if self.player_avail {
            let mut elements: Vec<&dyn I3BarWidget> = Vec::new();
            elements.push(&self.current_song);
            if let Some(ref prev) = self.prev {
                elements.push(prev);
            }
            if let Some(ref play) = self.play {
                elements.push(play);
            }
            if let Some(ref next) = self.next {
                elements.push(next);
            }
            elements
        } else if self.current_song.is_empty() {
            vec![&self.on_collapsed_click_widget]
        } else {
            vec![&self.current_song]
        }
    }
}

fn extract_artist_from_value(value: &dyn arg::RefArg) -> Result<&str> {
    if let Some(artist) = value.as_str() {
        Ok(artist)
    } else {
        extract_artist_from_value(
            value
                .as_iter()
                .block_error("music", "failed to extract artist")?
                .next()
                .block_error("music", "failed to extract artist")?,
        )
    }
}

#[allow(clippy::borrowed_box)] // TODO: remove clippy workaround
fn extract_from_metadata(metadata: &Box<dyn arg::RefArg>) -> Result<(String, String)> {
    let mut title = String::new();
    let mut artist = String::new();

    let mut iter = metadata
        .as_iter()
        .block_error("music", "failed to extract metadata")?;

    while let Some(key) = iter.next() {
        let value = iter
            .next()
            .block_error("music", "failed to extract metadata")?;
        match key
            .as_str()
            .block_error("music", "failed to extract metadata")?
        {
            "xesam:artist" => artist = String::from(extract_artist_from_value(value)?),
            "xesam:title" => {
                title = String::from(
                    value
                        .as_str()
                        .block_error("music", "failed to extract metadata")?,
                )
            }
            _ => {}
        };
    }
    Ok((title, artist))
}

fn get_first_available_player(
    connection: &Connection,
    interface_name_exclude_regexps: &Vec<Regex>,
) -> Option<String> {
    let m = Message::new_method_call(
        "org.freedesktop.DBus",
        "/",
        "org.freedesktop.DBus",
        "ListNames",
    )
    .unwrap();
    let r = connection.send_with_reply_and_block(m, 2000).unwrap();
    // ListNames returns one argument, which is an array of strings.
    let arr: Array<&str, _> = r.get1().unwrap();
    let mut names = Vec::new();
    for name in arr {
        // If an interface matches an exclude pattern, ignore it
        if interface_name_exclude_regexps
            .iter()
            .any(|regex| regex.is_match(&name))
        {
            continue;
        }

        if name.starts_with("org.mpris.MediaPlayer2") {
            names.push(String::from(name));
        }
    }

    if let Some(name) = names
        .iter()
        .find(|entry| entry.starts_with("org.mpris.MediaPlayer2"))
    {
        Some(String::from(name))
    } else {
        None
    }
}
