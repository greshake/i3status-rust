use std::boxed::Box;
use std::result;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::{
    arg::{Array, RefArg},
    ffidisp::stdintf::org_freedesktop_dbus::{Properties, PropertiesPropertiesChanged},
    ffidisp::{BusType, Connection},
    message::SignalArgs,
    Message,
};
use regex::Regex;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::{LogicalDirection, Scrolling, SharedConfig};
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::util::pseudo_uuid;
use crate::widgets::{
    rotatingtext::RotatingTextWidget, text::TextWidget, I3BarWidget, Spacing, State,
};

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

impl Player {
    pub fn new(dbus_conn: &Connection, name: &str, bus_name: &str) -> Self {
        let path = dbus_conn.with_path(name, "/org/mpris/MediaPlayer2", 500);
        let data = path
            .get("org.mpris.MediaPlayer2.Player", "Metadata")
            .map(|d: Box<dyn RefArg>| extract_from_metadata(d.as_ref()));
        let (title, artist) = match data {
            Ok(Ok(res)) => res,
            _ => (None, None),
        };

        // Get current playback status
        let status = path
            .get("org.mpris.MediaPlayer2.Player", "PlaybackStatus")
            .map(|d: Box<dyn RefArg>| extract_playback_status(d.as_ref()))
            .unwrap_or_default();

        Self {
            bus_name: bus_name.to_string(),
            interface_name: name.to_string(),
            playback_status: status,
            artist,
            title,
        }
    }
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
    prev: Option<TextWidget>,
    play: Option<TextWidget>,
    next: Option<TextWidget>,
    on_collapsed_click_widget: TextWidget,
    on_collapsed_click: Option<String>,
    on_click: Option<String>,
    dbus_conn: Connection,
    marquee: bool,
    marquee_interval: Duration,
    smart_trim: bool,
    max_width: usize,
    separator: String,
    seek_step: i64,
    players: Arc<Mutex<Vec<Player>>>,
    hide_when_empty: bool,
    send: Sender<Task>,
    format: FormatTemplate,
    scrolling: Scrolling,
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

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct MusicConfig {
    /// Name of the music player. Must be the same name the player is
    /// registered with the MediaPlayer2 Interface. If not specified then
    /// the block will track all players found.
    pub player: Option<String>,

    /// Max width of the block in characters, not including the buttons.
    pub max_width: usize,

    /// Bool to specify whether the block will change width depending on the
    /// text content or remain static always (= max_width)
    pub dynamic_width: bool,

    /// Bool to specify if a marquee style rotation should be used if the
    /// title + artist is longer than max-width
    pub marquee: bool,

    /// Marquee interval in seconds. This is the delay between each rotation.
    #[serde(deserialize_with = "deserialize_duration")]
    pub marquee_interval: Duration,

    /// Marquee speed in seconds. This is the scrolling time used per character.
    #[serde(deserialize_with = "deserialize_duration")]
    pub marquee_speed: Duration,

    /// Bool to specify whether smart trimming should be used when marquee
    /// rotation is disabled and the title + artist is longer than
    /// max-width. It will trim from both the artist and the title in proportion
    /// to their lengths, to try and show the most information possible.
    pub smart_trim: bool,

    /// Separator to use between artist and title.
    pub separator: String,

    /// Array of control buttons to be displayed. Options are prev (previous title),
    /// play (play/pause) and next (next title).
    pub buttons: Vec<String>,

    pub on_collapsed_click: Option<String>,

    // Number of microseconds to seek forward/backward when scrolling on the bar.
    pub seek_step: i64,

    /// MPRIS interface name regex patterns to ignore.
    pub interface_name_exclude: Vec<String>,

    pub hide_when_empty: bool,

    /// Format string for displaying music player info.
    pub format: FormatTemplate,
}

impl Default for MusicConfig {
    fn default() -> Self {
        Self {
            player: None,
            max_width: 21,
            dynamic_width: false,
            marquee: true,
            marquee_interval: Duration::from_secs(10),
            marquee_speed: Duration::from_millis(500),
            smart_trim: false,
            separator: " - ".to_string(),
            buttons: Vec::new(),
            on_collapsed_click: None,
            seek_step: 1000,
            interface_name_exclude: Vec::new(),
            hide_when_empty: false,
            format: FormatTemplate::default(),
        }
    }
}

impl ConfigBlock for Music {
    type Config = MusicConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        send: Sender<Task>,
    ) -> Result<Self> {
        let play_id = pseudo_uuid();
        let prev_id = pseudo_uuid();
        let next_id = pseudo_uuid();
        let collapsed_id = pseudo_uuid();

        let dbus_conn = Connection::get_private(BusType::Session)
            .block_error("music", "failed to establish D-Bus connection")?;

        let interface_name_exclude_regexps =
            compile_regexps(block_config.clone().interface_name_exclude)
                .block_error("music", "failed to parse exclude patterns")?;

        // ListNames returns one argument, which is an array of strings.
        let list_names = dbus_conn
            .send_with_reply_and_block(
                Message::new_method_call(
                    "org.freedesktop.DBus",
                    "/",
                    "org.freedesktop.DBus",
                    "ListNames",
                )
                .unwrap(),
                500,
            )
            .unwrap();
        let names = list_names.get1::<Array<&str, _>>().unwrap().filter(|name| {
            // If an interface matches an exclude pattern, ignore it
            !ignored_player(
                name,
                &interface_name_exclude_regexps,
                block_config.player.clone(),
            )
        });

        let mut players = Vec::<Player>::new();
        for name in names {
            // Get bus connection name
            let get_name_owner = dbus_conn
                .send_with_reply_and_block(
                    Message::new_method_call(
                        "org.freedesktop.DBus",
                        "/",
                        "org.freedesktop.DBus",
                        "GetNameOwner",
                    )
                    .unwrap()
                    .append1(name),
                    500,
                )
                .unwrap();
            let bus_name: &str = get_name_owner.read1().unwrap();

            // Skip if already added
            if players.iter().any(|p| p.bus_name == bus_name) {
                continue;
            }

            // Add player
            players.push(Player::new(&dbus_conn, name, bus_name));
        }

        let players = Arc::new(Mutex::new(players));
        let players_clone = players.clone();
        let send_clone = send.clone();
        let preferred_player = block_config.player.clone();

        thread::Builder::new()
            .name("music".into())
            .spawn(move || {
                let dbus_conn = Connection::get_private(BusType::Session).unwrap();

                // Listen to changes of players
                dbus_conn.add_match("interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',path='/org/mpris/MediaPlayer2'").unwrap();
                // Add/remove players
                dbus_conn.add_match("interface='org.freedesktop.DBus',member='NameOwnerChanged',path='/org/freedesktop/DBus',arg0namespace='org.mpris.MediaPlayer2'").unwrap();

                // Skip the NameAcquired event.
                dbus_conn.incoming(10_000).next();

                loop {
                    for ref signal in dbus_conn.incoming(60_000) {
                        let mut players = players_clone
                            .lock()
                            .expect("failed to acquire lock for `players`");
                        let mut updated = false;

                        // Some property changed
                        if let Some(prop_changed) = PropertiesPropertiesChanged::from_message(signal) {
                            if let Some(sender) = signal.sender() {
                                let sender = sender.to_string();
                                if let Some(player) = players.iter_mut().find(|p| p.bus_name == sender) {
                                    if let Some(data) = prop_changed.changed_properties.get("Metadata") {
                                        let (title, artist) = extract_from_metadata(&data.0).unwrap_or((None,None));
                                        if player.title != title || player.artist != artist {
                                            player.title = title;
                                            player.artist = artist;
                                            updated = true;
                                        }
                                    }
                                    if let Some(data) = prop_changed.changed_properties.get("PlaybackStatus") {
                                        let new_playback = extract_playback_status(&data.0);
                                        if player.playback_status != new_playback {
                                            player.playback_status = new_playback;
                                            updated = true;
                                        }
                                    }
                                    // workaround for `playerctld`
                                    // This block keeps track of players currently active on the MPRIS bus,
                                    // and only clears the metadata when a player has disappeared from the bus.
                                    // However `playerctl` is essentially doing the same thing as this block by
                                    // keeping track of players by itself, and when the last player is closed
                                    // the playerctld bus still remains which means the block never clears the
                                    // metadata for the last player that disappeared. We can get around this by
                                    // listening to the PlayerNames signal sent by playerctld and then only clear
                                    // the metadata when there are no more players left.
                                    if let Some(data) = prop_changed.changed_properties.get("PlayerNames") {
                                        if data.0.as_iter().unwrap().peekable().peek().is_none() {
                                            player.artist = None;
                                            player.title = None;
                                            updated = true;
                                        }
                                    }
                                }
                            }
                        }
                        // Add/remove player
                        else if signal.member().as_deref() == Some("NameOwnerChanged") {
                            if let Ok((name, old_owner, new_owner)) = signal.read3::<&str, &str, &str>() {
                                match (old_owner, new_owner) {
                                    ("", new_owner) => { // Add a new player
                                        // Skip if already presented (or ignored)
                                        if !players.iter().any(|p| p.bus_name == new_owner) && !ignored_player(name, &interface_name_exclude_regexps, preferred_player.clone()) {
                                            players.push(Player::new(&dbus_conn,name,new_owner));
                                            updated = true;
                                        }
                                    }
                                    (old_owner, "") => { // Remove an old player
                                        if let Some(pos) = players.iter().position(|p| p.bus_name == old_owner) {
                                            players.remove(pos);
                                            updated = true;
                                        }
                                    }
                                    _ => ()
                                }
                            }
                        }

                        // Request to update the block
                        if updated {
                            send_clone.send(Task {
                                id,
                                update_time: Instant::now(),
                            }).unwrap();
                        }
                    }
                }
            })
            .unwrap();

        let mut play: Option<TextWidget> = None;
        let mut prev: Option<TextWidget> = None;
        let mut next: Option<TextWidget> = None;
        for button in block_config.buttons {
            match &*button {
                "play" => {
                    play = Some(
                        TextWidget::new(id, play_id, shared_config.clone())
                            .with_icon("music_play")?
                            .with_state(State::Info)
                            .with_spacing(Spacing::Hidden),
                    )
                }
                "next" => {
                    next = Some(
                        TextWidget::new(id, next_id, shared_config.clone())
                            .with_icon("music_next")?
                            .with_state(State::Info)
                            .with_spacing(Spacing::Hidden),
                    )
                }
                "prev" => {
                    prev = Some(
                        TextWidget::new(id, prev_id, shared_config.clone())
                            .with_icon("music_prev")?
                            .with_state(State::Info)
                            .with_spacing(Spacing::Hidden),
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
            patterns.iter().map(|p| Regex::new(p)).collect()
        }

        Ok(Music {
            id,
            play_id,
            prev_id,
            next_id,
            collapsed_id,
            current_song_widget: RotatingTextWidget::new(
                id,
                id,
                Duration::new(block_config.marquee_interval.as_secs(), 0),
                Duration::new(0, block_config.marquee_speed.subsec_nanos()),
                block_config.max_width,
                block_config.dynamic_width,
                shared_config.clone(),
            )
            .with_icon("music")?
            .with_state(State::Info)
            .with_spacing(Spacing::Hidden),
            prev,
            play,
            next,
            on_click: None,
            on_collapsed_click_widget: TextWidget::new(id, collapsed_id, shared_config.clone())
                .with_icon("music")?
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
            players,
            hide_when_empty: block_config.hide_when_empty,
            send,
            format: block_config.format.with_default("{combo}")?,
            scrolling: shared_config.scrolling,
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
            "artist" => Value::from_string(artist.clone()),
            "title" => Value::from_string(title.clone()),
            "combo" => Value::from_string(combo),
            //TODO
            //"vol" => volume,
            "player" => Value::from_string(player_name),
            "avail" => Value::from_string(players.len().to_string()),
        );

        if !(rotation_in_progress) {
            if title.is_empty() && artist.is_empty() {
                self.current_song_widget.set_text(String::new());
            } else {
                self.current_song_widget
                    .set_text(self.format.render(&values)?.0);
            }
        }

        let state = match metadata.playback_status {
            PlaybackStatus::Playing => State::Info,
            _ => State::Idle,
        };

        [&mut self.play, &mut self.prev, &mut self.next]
            .iter_mut()
            .filter_map(|button| button.as_mut())
            .for_each(|button| button.set_state(state));

        self.current_song_widget.set_state(state);

        if let Some(ref mut play) = self.play {
            play.set_icon(match metadata.playback_status {
                PlaybackStatus::Playing => "music_pause",
                PlaybackStatus::Paused => "music_play",
                PlaybackStatus::Stopped => "music_play",
                PlaybackStatus::Unknown => "music_play",
            })?
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
        if let Some(event_id) = event.instance {
            let action = match event_id {
                id if id == self.play_id => "PlayPause",
                id if id == self.next_id => "Next",
                id if id == self.prev_id => "Previous",
                id if id == self.id => "",
                id if id == self.collapsed_id => "",
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
                        match self.scrolling.to_logical_direction(event.button) {
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
        if players.len() <= 1 && self.current_song_widget.is_empty() && self.hide_when_empty {
            vec![]
        } else if players.len() > 0 && !self.current_song_widget.is_empty() {
            let mut elements: Vec<&dyn I3BarWidget> = vec![&self.current_song_widget];
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

fn extract_from_metadata(metadata: &dyn RefArg) -> Result<(Option<String>, Option<String>)> {
    let mut title = None;
    let mut artist = None;

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
            "xesam:artist" => artist = Some(String::from(extract_artist_from_value(value)?)),
            "xesam:title" => {
                title = Some(String::from(
                    value
                        .as_str()
                        .block_error("music", "failed to extract metadata")?,
                ))
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
        .any(|regex| regex.is_match(name))
    {
        return true;
    }

    false
}
