use std::boxed::Box;
use std::collections::BTreeMap;
use std::result;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::{
    arg::{Array, RefArg},
    ffidisp::stdintf::org_freedesktop_dbus::{Properties, PropertiesPropertiesChanged},
    ffidisp::{BusType, Connection, ConnectionItem},
    message::SignalArgs,
    Message,
};
use regex::Regex;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::{Config, LogicalDirection};
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::widget::{I3BarWidget, State};
use crate::widgets::button::ButtonWidget;
use crate::widgets::rotatingtext::RotatingTextWidget;

#[derive(Debug, Clone)]
struct Player {
    interface_name: String,
    playback_status: PlaybackStatus,
    artist: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
    Unknown,
}

impl Default for PlaybackStatus {
    fn default() -> Self {
        PlaybackStatus::Unknown
    }
}

pub struct Music {
    id: String,
    current_song_widget: RotatingTextWidget,
    prev: Option<ButtonWidget>,
    play: Option<ButtonWidget>,
    next: Option<ButtonWidget>,
    on_collapsed_click_widget: ButtonWidget,
    on_collapsed_click: Option<String>,
    on_click: Option<String>,
    dbus_conn: Connection,
    marquee: bool,
    marquee_interval: Duration,
    smart_trim: bool,
    max_width: usize,
    separator: String,
    seek_step: i64,
    config: Config,
    players: Arc<Mutex<BTreeMap<String, Player>>>,
    hide_when_empty: bool,
}

impl Music {
    fn smart_trim(&self, artist: String, title: String) -> String {
        // Below code is by https://github.com/jgbyrne
        let mut artist: String = artist;
        let mut title: String = title;
        let textlen =
            title.chars().count() + self.separator.chars().count() + artist.chars().count();

        if title.is_empty() {
            artist.truncate(self.max_width);
        } else if artist.is_empty() {
            title.truncate(self.max_width);
        } else {
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
        }
        format!("{}{}{}", title, self.separator, artist)
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct MusicConfig {
    /// Name of the music player. Must be the same name the player is
    /// registered with the MediaPlayer2 Interface. If not specified then
    /// auto-discovery of currently active player.
    pub player: Option<String>,

    /// Max width of the block in characters, not including the buttons.
    #[serde(default = "MusicConfig::default_max_width")]
    pub max_width: usize,

    /// Bool to specify whether the block will change width depending on the
    /// text content or remain static always (= max_width)
    #[serde(default = "MusicConfig::default_dynamic_width")]
    pub dynamic_width: bool,

    /// Bool to specify if a marquee style rotation should be used if the
    /// title + artist is longer than max-width
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

    /// Bool to specify whether smart trimming should be used when marquee
    /// rotation is disabled and the title + artist is longer than
    /// max-width. It will trim from both the artist and the title in proportion
    /// to their lengths, to try and show the most information possible.
    #[serde(default = "MusicConfig::default_smart_trim")]
    pub smart_trim: bool,

    /// Separator to use between artist and title.
    #[serde(default = "MusicConfig::default_separator")]
    pub separator: String,

    /// Array of control buttons to be displayed. Options are prev (previous title),
    /// play (play/pause) and next (next title).
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
        let id_copy3 = id.clone();
        let send2 = send.clone();

        let c = Connection::get_private(BusType::Session)
            .block_error("music", "failed to establish D-Bus connection")?;

        let interface_name_exclude_regexps =
            compile_regexps(block_config.clone().interface_name_exclude)
                .block_error("music", "failed to parse exclude patterns")?;

        let mut initial_players = BTreeMap::new();
        let m = Message::new_method_call(
            "org.freedesktop.DBus",
            "/",
            "org.freedesktop.DBus",
            "ListNames",
        )
        .unwrap();
        let r = c.send_with_reply_and_block(m, 500).unwrap();
        // ListNames returns one argument, which is an array of strings.
        let arr: Array<&str, _> = r.get1().unwrap();
        for name in arr {
            // TODO: prefilter arr before entering loop
            // If an interface matches an exclude pattern, ignore it
            if ignored_player(
                &name,
                &interface_name_exclude_regexps,
                block_config.clone().player,
            ) {
                continue;
            }

            // Get bus connection name
            // TODO: possibly could get this info from the sender field of the Metadata call below?
            let m = Message::new_method_call(
                "org.freedesktop.DBus",
                "/",
                "org.freedesktop.DBus",
                "GetNameOwner",
            )
            .unwrap();
            let r = c.send_with_reply_and_block(m.append1(name), 500).unwrap();
            let bn: &str = r.read1().ok().unwrap();

            // Get current media info, if any
            let p = c.with_path(name, "/org/mpris/MediaPlayer2", 500);
            let data = p.get("org.mpris.MediaPlayer2.Player", "Metadata");
            let (t, a) = match data {
                Err(_) => (String::new(), String::new()),
                Ok(data) => extract_from_metadata(&data).unwrap_or((String::new(), String::new())),
            };

            // Get current playback status
            let data = p.get("org.mpris.MediaPlayer2.Player", "PlaybackStatus");
            let status = match data {
                Err(_) => PlaybackStatus::Unknown,
                Ok(data) => {
                    let data: Box<dyn RefArg> = data;
                    extract_playback_status(&data)
                }
            };

            initial_players.insert(
                bn.to_string(),
                Player {
                    interface_name: name.to_string(),
                    playback_status: status,
                    artist: Some(a),
                    title: Some(t),
                },
            );
        }

        let players_original = Arc::new(Mutex::new(initial_players));
        let players_copy = players_original.clone();
        let players_copy2 = players_original.clone();
        let players_copy3 = players_original;
        thread::Builder::new().name("music".into()).spawn(move || {
            let c = Connection::get_private(BusType::Session).unwrap();
            c.add_match("interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',path='/org/mpris/MediaPlayer2'").unwrap();
            loop {
                for msg in c.incoming(100_000) {
                    // We are listening to events from all players on org.mpris.MediaPlayer2,
                    // but we only want to update for our currently selected player (either
                    // set by the user in the config file, or autodiscovered by us).
                    if msg.sender().is_some() {
                        if let Some(signal) = PropertiesPropertiesChanged::from_message(&msg) {
                            let mut players = players_copy2.lock().expect("failed to acquire lock for `players`");
                            let players_copy = players.clone();
                            let player = players_copy.get(&msg.sender().unwrap().to_string());
                            if player.is_none() {
                                // Ignoring update since players array was empty.
                                // This shouldn't actually occur as long as the other thread updates in time.
                                continue;
                            }
                            let mut new = player.unwrap().clone();
                            let mut updated = false;
                            let raw_metadata = signal.changed_properties.get("Metadata");
                            if let Some(data) = raw_metadata {
                                let (title, artist) =
                                    extract_from_metadata(&data.0).unwrap_or((String::new(), String::new()));
                                if new.artist != Some(artist.clone()) {
                                       new.artist = Some(artist);
                                       updated = true;
                                }
                                if new.title != Some(title.clone()) {
                                    new.title = Some(title);
                                       updated = true;
                                }
                            };
                            let raw_metadata = signal.changed_properties.get("PlaybackStatus");
                            if let Some(data) = raw_metadata {
                                let new_status = extract_playback_status(&data.0);
                                if new.playback_status != new_status {
                                    new.playback_status = new_status;
                                    updated = true;
                                }
                            };
                            if updated {
                                players.insert(msg.sender().unwrap().to_string(), new);
                                send.send(Task {
                                    id: id.clone(),
                                    update_time: Instant::now(),
                                })
                                .unwrap();
                            }
                        }
                    }
                }
            }
        }).unwrap();

        // Some players do not seem to update their Metadata on close which leads to the block showing old info.
        // To fix this we will the bus to see when players have disappeared so that we can schedule a block update.
        let preferred_player = block_config.clone().player;
        thread::Builder::new().name("music".into()).spawn(move || {
            let c = Connection::get_private(BusType::Session).unwrap();
            c.add_match("interface='org.freedesktop.DBus',member='NameOwnerChanged',path='/org/freedesktop/DBus',arg0namespace='org.mpris.MediaPlayer2'")
                .unwrap();
            // Skip the NameAcquired event.
            c.incoming(10_000).next();
            loop {
                for ci in c.iter(100_000) {
                    if let ConnectionItem::Signal(x) = ci {
                         let (name, old_owner, new_owner): (&str, &str, &str) = x.read3().unwrap();
                         let mut players = players_copy3.lock().expect("failed to acquire lock for `players`");
                         if !old_owner.is_empty() && new_owner.is_empty() {
                             players.remove(old_owner);
                             send2.send(Task {
                                 id: id_copy3.clone(),
                                 update_time: Instant::now(),
                             })
                             .unwrap();
                         } else if old_owner.is_empty() && !new_owner.is_empty() && !ignored_player(name, &interface_name_exclude_regexps, preferred_player.clone()) {
                             players.insert(new_owner.to_string(), Player {
                                 interface_name: name.to_string(),
                                 playback_status: PlaybackStatus::Unknown,
                                 artist: None,
                                 title: None,
                             });
                             send2.send(Task {
                                 id: id_copy3.clone(),
                                 update_time: Instant::now(),
                             })
                             .unwrap();
                        }
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
            current_song_widget: RotatingTextWidget::new(
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
            marquee: block_config.marquee,
            marquee_interval: block_config.marquee_interval,
            smart_trim: block_config.smart_trim,
            max_width: block_config.max_width,
            separator: block_config.separator,
            seek_step: block_config.seek_step,
            config,
            players: players_copy,
            hide_when_empty: block_config.hide_when_empty,
        })
    }
}

impl Block for Music {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let (rotation_in_progress, time_to_next_rotation) = if self.marquee {
            self.current_song_widget.next()?
        } else {
            (false, None)
        };

        let players = self
            .players
            .lock()
            .block_error("music", "failed to acquire lock for `players`")?;
        // get first player
        let (_busname, metadata) = match players.iter().next() {
            Some((k, v)) => (k, v),
            None => {
                self.current_song_widget.set_text(String::from(""));
                return Ok(None);
            }
        };

        let artist = metadata.clone().artist.unwrap_or_else(|| String::from(""));
        let title = metadata.clone().title.unwrap_or_else(|| String::from(""));

        if !(rotation_in_progress) {
            if title.is_empty() && artist.is_empty() {
                self.current_song_widget.set_text(String::new());
            } else if (title.chars().count()
                + self.separator.chars().count()
                + artist.chars().count())
                < self.max_width
                || !self.smart_trim
            {
                self.current_song_widget
                    .set_text(format!("{}{}{}", title, self.separator, artist));
            } else {
                self.current_song_widget
                    .set_text(self.smart_trim(artist, title));
            }
        }

        if let Some(ref mut play) = self.play {
            play.set_icon(match metadata.playback_status {
                PlaybackStatus::Playing => "music_pause",
                PlaybackStatus::Paused => "music_play",
                PlaybackStatus::Stopped => "music_play",
                PlaybackStatus::Unknown => "music_play",
            })
        }

        // If `marquee` is enabled then we need to schedule an update for the text rotation.
        // (time_to_next_rotation is always None if marquee is disabled)
        if let Some(t) = time_to_next_rotation {
            Ok(Some(Update::Every(t)))
        // We just finished a rotation so we wait before starting again
        } else if self.marquee {
            Ok(Some(self.marquee_interval.into()))
        // Otherwise we do not need to schedule anything as the block will auto-update itself after
        // seeing a PropertiesChanged signal for the MPRIS interface it is monitoring.
        } else {
            Ok(None)
        }
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = event.name {
            let action = match name as &str {
                "play" => "PlayPause",
                "next" => "Next",
                "prev" => "Previous",
                _ => "",
            };

            let players = self
                .players
                .lock()
                .block_error("music", "failed to acquire lock for `players`")?;
            // get first player
            let (_busname, metadata) = players.iter().next().unwrap();

            match event.button {
                MouseButton::Left => {
                    if action != "" {
                        let m = Message::new_method_call(
                            metadata.interface_name.clone(),
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
                // TODO: on right mouse click we can cycle through the current players
                _ => {
                    if name.as_str() == self.id {
                        let m = Message::new_method_call(
                            metadata.interface_name.clone(),
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
        let players = self
            .players
            .lock()
            .expect("failed to acquire lock for `players`");
        if players.len() == 0 && self.hide_when_empty {
            vec![]
        } else if players.len() > 0 && !self.current_song_widget.is_empty() {
            let mut elements: Vec<&dyn I3BarWidget> = Vec::new();
            elements.push(&self.current_song_widget);
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
        } else if self.current_song_widget.is_empty() {
            vec![&self.on_collapsed_click_widget]
        } else {
            vec![&self.current_song_widget]
        }
    }
}

fn extract_playback_status(value: &dyn RefArg) -> PlaybackStatus {
    if let Some(status) = value.as_str() {
        match status {
            "Playing" => PlaybackStatus::Playing,
            "Paused" => PlaybackStatus::Paused,
            "Stopped" => PlaybackStatus::Stopped,
            _ => PlaybackStatus::Unknown,
        }
    } else {
        PlaybackStatus::Unknown
    }
}

fn extract_artist_from_value(value: &dyn RefArg) -> Result<&str> {
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
fn extract_from_metadata(metadata: &Box<dyn RefArg>) -> Result<(String, String)> {
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

fn ignored_player(
    name: &str,
    interface_name_exclude_regexps: &[Regex],
    preferred_player: Option<String>,
) -> bool {
    // If the player is specified in the config then we will ignore all others.
    if let Some(p) = preferred_player {
        if !name.starts_with(&format!("org.mpris.MediaPlayer2.{}", p)) {
            return true;
        }
    }

    if !name.starts_with("org.mpris.MediaPlayer2") {
        return true;
    }

    if interface_name_exclude_regexps
        .iter()
        .any(|regex| regex.is_match(&name))
    {
        return true;
    }

    false
}
