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

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::{Config, LogicalDirection};
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::util::{pseudo_uuid, FormatTemplate};
use crate::widget::{I3BarWidget, Spacing, State};
use crate::widgets::button::ButtonWidget;
use crate::widgets::rotatingtext::RotatingTextWidget;

#[derive(Debug, Clone)]
struct Player {
    bus_name: String,
    interface_name: String,
    playback_status: PlaybackStatus,
    artist: Option<String>,
    title: Option<String>,
    //TODO
    //volume: u32,
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
    id: usize,
    play_id: usize,
    next_id: usize,
    prev_id: usize,
    collapsed_id: usize,

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
    players: Arc<Mutex<Vec<Player>>>,
    hide_when_empty: bool,
    send: Sender<Task>,
    format: FormatTemplate,
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
            if !(1..5001).contains(&ttrc) {
                ttrc = 1
            }

            let mut atrc = alen - anum;
            if !(1..5001).contains(&atrc) {
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

    // Number of microseconds to seek forward/backward when scrolling on the bar.
    #[serde(default = "MusicConfig::default_seek_step")]
    pub seek_step: i64,

    /// MPRIS interface name regex patterns to ignore.
    #[serde(default = "MusicConfig::default_interface_name_exclude_patterns")]
    pub interface_name_exclude: Vec<String>,

    #[serde(default = "MusicConfig::default_hide_when_empty")]
    pub hide_when_empty: bool,

    /// Format string for displaying music player info.
    #[serde(default = "MusicConfig::default_format")]
    pub format: String,

    #[serde(default = "MusicConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
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

    fn default_seek_step() -> i64 {
        1000
    }

    fn default_interface_name_exclude_patterns() -> Vec<String> {
        vec![]
    }

    fn default_hide_when_empty() -> bool {
        false
    }

    fn default_format() -> String {
        "{combo}".to_string()
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}

impl ConfigBlock for Music {
    type Config = MusicConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        config: Config,
        send: Sender<Task>,
    ) -> Result<Self> {
        let play_id = pseudo_uuid();
        let prev_id = pseudo_uuid();
        let next_id = pseudo_uuid();
        let collapsed_id = pseudo_uuid();

        let send2 = send.clone();
        let send3 = send.clone();

        let c = Connection::get_private(BusType::Session)
            .block_error("music", "failed to establish D-Bus connection")?;

        let interface_name_exclude_regexps =
            compile_regexps(block_config.clone().interface_name_exclude)
                .block_error("music", "failed to parse exclude patterns")?;

        let mut initial_players: Vec<Player> = Vec::new();
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

            if !initial_players.iter().any(|p| p.bus_name == bn) {
                // Get current media info, if any
                let p = c.with_path(name, "/org/mpris/MediaPlayer2", 500);
                let data = p.get("org.mpris.MediaPlayer2.Player", "Metadata");
                let (title, artist) = match data {
                    Err(_) => (String::new(), String::new()),
                    Ok(data) => {
                        extract_from_metadata(&data).unwrap_or((String::new(), String::new()))
                    }
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

                initial_players.push(Player {
                    bus_name: bn.to_string(),
                    interface_name: name.to_string(),
                    playback_status: status,
                    artist: Some(artist),
                    title: Some(title),
                });
            }
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
                            let player = players.iter_mut().find(|p| p.bus_name == msg.sender().unwrap().to_string());
                            if player.is_none() {
                                // Ignoring update since could not find player in the array.
                                // This shouldn't actually occur as long as the other thread updates the array in time.
                                continue;
                            }
                            let p = player.unwrap();
                            let mut updated = false;
                            let raw_metadata = signal.changed_properties.get("Metadata");
                            if let Some(data) = raw_metadata {
                                let (title, artist) =
                                    extract_from_metadata(&data.0).unwrap_or((String::new(), String::new()));
                                if p.artist != Some(artist.clone()) {
                                    p.artist = Some(artist);
                                    updated = true;
                                }
                                if p.title != Some(title.clone()) {
                                    p.title = Some(title);
                                    updated = true;
                                }
                            };
                            let raw_metadata = signal.changed_properties.get("PlaybackStatus");
                            if let Some(data) = raw_metadata {
                                let new_status = extract_playback_status(&data.0);
                                if p.playback_status != new_status {
                                    p.playback_status = new_status;
                                    updated = true;
                                }
                            };
                            // workaround for `playerctld`
                            // This block keeps track of players currently activeon the MPRIS bus, 
                            // and only clears the metadata when a player has disappeared from the bus.
                            // However `playerctl` is essentially doing the same thing as this block by
                            // keeping track of players by itself, and when the last player is closed
                            // the playerctld bus still remains which means the block never clears the
                            // metadata for the last player that disappeared. We can get around this by
                            // listening to the PlayerNames signal sent by playerctld and then only clear 
                            // the metadata when there are no more players left.
                            let raw_metadata = signal.changed_properties.get("PlayerNames");
                            if let Some(data) = raw_metadata {
                                let mut playerctl_playerlist = data.0.as_iter().unwrap().peekable();
                                if playerctl_playerlist.peek().is_none() {
                                    p.artist = None;
                                    p.title = None;
                                    updated = true;
                                }
                            };
                            if updated {
                                send.send(Task {
                                    id,
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
                             if let Some(pos) = players.iter().position(|p| p.bus_name == old_owner) {
                                 players.remove(pos);
                                 send2.send(Task {
                                     id,
                                     update_time: Instant::now(),
                                 })
                                 .unwrap();
                             }
                         } else if old_owner.is_empty() && !new_owner.is_empty() && !ignored_player(name, &interface_name_exclude_regexps, preferred_player.clone()) && !players.iter().any(|p| p.bus_name == new_owner) {
                         players.push(Player {
                             bus_name: new_owner.to_string(),
                             interface_name: name.to_string(),
                             playback_status: PlaybackStatus::Unknown,
                             artist: None,
                             title: None,
                         });
                         send2.send(Task {
                             id,
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
                        ButtonWidget::new(config.clone(), play_id)
                            .with_icon("music_play")
                            .with_state(State::Info)
                            .with_spacing(Spacing::Inline),
                    )
                }
                "next" => {
                    next = Some(
                        ButtonWidget::new(config.clone(), next_id)
                            .with_icon("music_next")
                            .with_state(State::Info)
                            .with_spacing(Spacing::Inline),
                    )
                }
                "prev" => {
                    prev = Some(
                        ButtonWidget::new(config.clone(), prev_id)
                            .with_icon("music_prev")
                            .with_state(State::Info)
                            .with_spacing(Spacing::Inline),
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
            id,
            play_id,
            prev_id,
            next_id,
            collapsed_id,
            current_song_widget: RotatingTextWidget::new(
                Duration::new(block_config.marquee_interval.as_secs(), 0),
                Duration::new(0, block_config.marquee_speed.subsec_nanos()),
                block_config.max_width,
                block_config.dynamic_width,
                config.clone(),
                id,
            )
            .with_icon("music")
            .with_state(State::Info),
            prev,
            play,
            next,
            on_click: None,
            on_collapsed_click_widget: ButtonWidget::new(config.clone(), collapsed_id)
                .with_icon("music")
                .with_state(State::Info)
                .with_spacing(Spacing::Hidden),
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
            send: send3,
            format: FormatTemplate::from_string(&block_config.format)?,
        })
    }

    fn override_on_click(&mut self) -> Option<&mut Option<String>> {
        Some(&mut self.on_click)
    }
}

impl Block for Music {
    fn id(&self) -> usize {
        self.id
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
        let metadata = match players.first() {
            Some(m) => m,
            None => {
                self.current_song_widget.set_text(String::from(""));
                return Ok(None);
            }
        };

        let interface_name = metadata.clone().interface_name;
        let split: Vec<&str> = interface_name.split('.').collect();
        let player_name = split[3].to_string();
        let artist = metadata.clone().artist.unwrap_or_else(|| String::from(""));
        let title = metadata.clone().title.unwrap_or_else(|| String::from(""));
        let combo =
            if (title.chars().count() + self.separator.chars().count() + artist.chars().count())
                < self.max_width
                || !self.smart_trim
            {
                format!("{}{}{}", title, self.separator, artist)
            } else {
                self.smart_trim(artist.clone(), title.clone())
            };

        let values = map!(
            "{artist}" => artist.clone(),
            "{title}" => title.clone(),
            "{combo}" => combo,
            //TODO
            //"{vol}" => volume,
            "{player}" => player_name,
            "{avail}" => players.len().to_string()
        );

        if !(rotation_in_progress) {
            if title.is_empty() && artist.is_empty() {
                self.current_song_widget.set_text(String::new());
            } else {
                self.current_song_widget
                    .set_text(self.format.render_static_str(&values)?);
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
        if let Some(event_id) = event.id {
            let action = match event_id {
                id if id == self.play_id => "PlayPause",
                id if id == self.next_id => "Next",
                id if id == self.prev_id => "Previous",
                id if id == self.id => "",
                _ => return Ok(()),
            };

            let mut players = self
                .players
                .lock()
                .block_error("music", "failed to acquire lock for `players`")?;

            match event.button {
                MouseButton::Left => {
                    if !action.is_empty() && players.len() > 0 {
                        let metadata = players.first().unwrap();
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
                    } else if event_id == self.collapsed_id && self.on_collapsed_click.is_some() {
                        let cmd = self.on_collapsed_click.as_ref().unwrap();
                        spawn_child_async("sh", &["-c", cmd])
                            .block_error("music", "could not spawn child")?;
                    } else if event_id == self.id {
                        if let Some(ref cmd) = self.on_click {
                            spawn_child_async("sh", &["-c", cmd])
                                .block_error("music", "could not spawn child")?;
                        }
                    }
                }
                // TODO(?): If there is only one player in the queue and it is playerctld,
                // then in that case send the "Shift" command via D-Bus to make playerctl
                // cycle to the next player. Then this block will also update automatically.
                // CLI cmd for reference (see "Seek" below for how to implement it in code):
                // busctl --user call org.mpris.MediaPlayer2.playerctld \
                //                    /org/mpris/MediaPlayer2 \
                //                    com.github.altdesktop.playerctld \
                //                    Shift
                MouseButton::Right => {
                    if (event_id == self.id || event_id == self.collapsed_id) && players.len() > 0 {
                        players.rotate_left(1);
                        self.send.send(Task {
                            id: self.id,
                            update_time: Instant::now(),
                        })?;
                    }
                }
                _ => {
                    if event_id == self.id && players.len() > 0 {
                        let metadata = players.first().unwrap();
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
        if players.len() == 1 && self.current_song_widget.is_empty() && self.hide_when_empty {
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
