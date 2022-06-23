//! The current song title and artist
//!
//! Also provides buttons for play/pause, previous and next.
//!
//! Supports all music players that implement the [MediaPlayer2 Interface]. This includes:
//!
//! - Spotify
//! - VLC
//! - mpd (via [mpDris2](https://github.com/eonpatapon/mpDris2))
//!
//! and many others.
//!
//! By default the block tracks all players available on the MPRIS bus. Right clicking on the block
//! will cycle it to the next player. You can pin the widget to a given player via the "player"
//! setting.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>"$combo.rot-str() $play&vert;"</code>
//! `player` | Name of the music player MPRIS interface. Run <code>busctl --user list &vert; grep "org.mpris.MediaPlayer2." &vert; cut -d' ' -f1</code> and the name is the part after "org.mpris.MediaPlayer2.". | `None`
//! `interface_name_exclude` | A list of regex patterns for player MPRIS interface names to ignore. | `[]`
//! `separator` | String to insert between artist and title. | `" - "`
//! `seek_step` | Number of microseconds to seek forward/backward when scrolling on the bar. | `1000`
//! `hide_when_empty` | Hides the block when there is no player available. | `false`
//!
//! Note: All placeholders can be absent. See the examples below to learn how to handle this.
//!
//! Placeholder | Value          | Type
//! ------------|----------------|------
//! `artist`    | Current artist | Text
//! `title`     | Current title  | Text
//! `url`       | Current song url | Text
//! `combo`     | Resolves to "`$artist[sep]$title"`, `"$artist"`, `"$title"`, or `"$url"` depending on what information is available. `[sep]` is set by `separator` option. | Text
//! `player`    | Name of the current player (taken from the last part of its MPRIS bus name) | Text
//! `avail`     | Total number of players available to switch between | Number
//! `cur`       | Total number of players available to switch between | Number
//! `play`      | Play/Pause button | Clickable icon
//! `next`      | Next button | Clickable icon
//! `prev`      | Previous button | Clickable icon
//!
//! # Examples
//!
//! Show the currently playing song on Spotify only, with play & next buttons and limit the width
//! to 20 characters:
//!
//! ```toml
//! [[block]]
//! block = "music"
//! format = "$combo.str(20) $play $next|"
//! player = "spotify"
//! ```
//!
//! Same thing for any compatible player, takes the first active on the bus, but ignores "mpd" or anything with "kdeconnect" in the name:
//!
//! ```toml
//! [[block]]
//! block = "music"
//! format = "$combo.str(20) $play $next|"
//! interface_name_exclude = [".*kdeconnect.*", "mpd"]
//! ```
//!
//! Same as above, but displays with rotating text
//!
//! ```toml
//! [[block]]
//! block = "music"
//! format = "$combo.rot-str(20) $play $next|"
//! interface_name_exclude = [".*kdeconnect.*", "mpd"]
//! ```
//!
//! # Icons Used
//! - `music`
//! - `music_next`
//! - `music_play`
//! - `music_prev`
//!
//! [MediaPlayer2 Interface]: https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html

use zbus::fdo::DBusProxy;
use zbus::names::{OwnedBusName, OwnedInterfaceName};
use zbus::zvariant::{Optional, OwnedValue, Type};
use zbus::MessageStream;

use regex::Regex;
use std::collections::HashMap;

use super::prelude::*;
mod zbus_mpris;

make_log_macro!(debug, "music");

const PLAY_PAUSE_BTN: usize = 1;
const NEXT_BTN: usize = 2;
const PREV_BTN: usize = 3;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
struct MusicConfig {
    format: FormatConfig,
    player: Option<String>,
    interface_name_exclude: Vec<String>,
    #[default(" - ".into())]
    separator: String,
    #[default(1_000)]
    seek_step: i64,
    hide_when_empty: bool,
}

#[derive(Debug, Clone, Type, Deserialize)]
struct PropChange {
    _interface_name: OwnedInterfaceName,
    changed_properties: HashMap<String, OwnedValue>,
    _invalidated_properties: Vec<String>,
}

#[derive(Debug, Clone, Type, Deserialize)]
struct OwnerChange {
    pub name: OwnedBusName,
    pub old_owner: Optional<String>,
    pub new_owner: Optional<String>,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = MusicConfig::deserialize(config).config_error()?;
    let dbus_conn = new_dbus_connection().await?;
    let mut widget = api
        .new_widget()
        .with_icon("music")?
        .with_format(config.format.with_default("$combo.rot-str() $play|")?);

    let new_btn = |icon: &str, id: usize, api: &mut CommonApi| -> Result<Value> {
        Ok(Value::icon(api.get_icon(icon)?).with_instance(id))
    };

    let values = map! {
        "next" => new_btn("music_next", NEXT_BTN, &mut api)?,
        "prev" => new_btn("music_prev", PREV_BTN, &mut api)?,
    };

    let fileter_name = config.player.as_deref();
    let exclude_regex = config
        .interface_name_exclude
        .iter()
        .map(|r| Regex::new(r))
        .collect::<Result<Vec<_>, _>>()
        .error("Invalid regex")?;

    let mut players = get_players(&dbus_conn, fileter_name, &exclude_regex).await?;
    let mut cur_player = None;
    for (i, player) in players.iter().enumerate() {
        cur_player = Some(i);
        if player.status == Some(PlaybackStatus::Playing) {
            break;
        }
    }

    let dbus_proxy = DBusProxy::new(&dbus_conn)
        .await
        .error("failed to cerate DBusProxy")?;
    dbus_proxy.add_match("type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',path='/org/mpris/MediaPlayer2'")
            .await
            .error( "failed to add match")?;
    dbus_proxy.add_match("type='signal',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0namespace='org.mpris.MediaPlayer2'")
            .await
            .error( "failed to add match")?;
    let mut dbus_stream = MessageStream::from(&dbus_conn);

    loop {
        debug!("available players:");
        for player in &players {
            debug!("{}", player.bus_name);
        }

        let avail = players.len();
        let player = cur_player.map(|c| players.get_mut(c).unwrap());
        match player {
            Some(ref player) => {
                let mut values = values.clone();
                values.insert("avail".into(), Value::number(avail));
                values.insert("cur".into(), Value::number(cur_player.unwrap() + 1));
                values.insert(
                    "player".into(),
                    Value::text(
                        extract_player_name(player.bus_name.as_str())
                            .unwrap()
                            .into(),
                    ),
                );
                let (state, play_icon) = match player.status {
                    Some(PlaybackStatus::Playing) => (State::Info, "music_pause"),
                    _ => (State::Idle, "music_play"),
                };
                values.insert("play".into(), new_btn(play_icon, PLAY_PAUSE_BTN, &mut api)?);
                if let Some(url) = &player.url {
                    values.insert("url".into(), Value::text(url.clone()));
                }
                match (&player.title, &player.artist, &player.url) {
                    (Some(t), None, _) => {
                        values.insert("combo".into(), Value::text(t.clone()));
                        values.insert("title".into(), Value::text(t.clone()));
                    }
                    (None, Some(a), _) => {
                        values.insert("combo".into(), Value::text(a.clone()));
                        values.insert("artist".into(), Value::text(a.clone()));
                    }
                    (Some(t), Some(a), _) => {
                        values.insert(
                            "combo".into(),
                            Value::text(format!("{t}{}{a}", config.separator)),
                        );
                        values.insert("title".into(), Value::text(t.clone()));
                        values.insert("artist".into(), Value::text(a.clone()));
                    }
                    (None, None, Some(url)) => {
                        values.insert("combo".into(), Value::text(url.clone()));
                    }
                    _ => (),
                }
                widget.set_values(values);
                widget.state = state;
                api.set_widget(&widget).await?;
            }
            None if config.hide_when_empty => {
                api.hide().await?;
            }
            None => {
                widget.set_values(default());
                widget.state = State::Idle;
                api.set_widget(&widget).await?;
            }
        }

        select! {
            // Wait for a DBUS event
            Some(msg) = dbus_stream.next() => {
                let msg = msg.unwrap();
                match msg.member().as_ref().map(|m| m.as_str()) {
                    Some("PropertiesChanged") => {
                        let header = msg.header().unwrap();
                        let sender = header.sender().unwrap().unwrap();
                        if let Some(player) = players.iter_mut().find(|p| p.owner == sender.to_string()) {
                            let body: PropChange = msg.body().unwrap();
                            let props = body.changed_properties;

                            if let Some(status) = props.get("PlaybackStatus") {
                                let status: &str = status.downcast_ref().unwrap();
                                player.status = PlaybackStatus::from_str(status);
                            }
                            if let Some(metadata) = props.get("Metadata") {
                                let metadata =
                                    zbus_mpris::PlayerMetadata::try_from(metadata.clone()).unwrap();
                                player.update_metadata(metadata);
                            }
                        }
                    }
                    Some("NameOwnerChanged") => {
                        let body: OwnerChange = msg.body().unwrap();
                        let old: Option<String> = body.old_owner.into();
                        let new: Option<String> = body.new_owner.into();
                        match (old, new) {
                            (None, Some(new)) => if new != body.name.to_string() && player_matches(body.name.as_str(), fileter_name, &exclude_regex) {
                                players.push(Player::new(&dbus_conn, body.name, new).await?);
                                cur_player = Some(players.len() - 1);
                            }
                            (Some(old), None) => {
                                if let Some(pos) = players.iter().position(|p| p.owner == old) {
                                    players.remove(pos);
                                    if let Some(cur) = cur_player {
                                        if players.is_empty() {
                                            cur_player = None;
                                        } else if pos == cur {
                                            cur_player = Some(0);
                                        } else if pos < cur {
                                            cur_player = Some(cur - 1);
                                        }
                                    }
                                }
                            }
                            _ => (),
                        }
                    }
                    _ => (),
                }
            }
            // Wait for a click
            Click(click) = api.event() => {
                if let Some(i) = cur_player {
                    match click.button {
                        MouseButton::Left => {
                            let player = &mut players[i];
                            match click.instance {
                                Some(PLAY_PAUSE_BTN) => player.play_pause().await?,
                                Some(NEXT_BTN) => player.next().await?,
                                Some(PREV_BTN) => player.prev().await?,
                                _ => (),
                            }
                        }
                        MouseButton::Right => {
                            if i + 1 < players.len() {
                                cur_player = Some(i + 1);
                            } else {
                                cur_player = Some(0);
                            }
                        }
                        MouseButton::WheelUp => {
                            players[i].seek(config.seek_step).await?;
                        }
                        MouseButton::WheelDown => {
                            players[i].seek(-config.seek_step).await?;
                        }
                        _ => (),
                    }
                }
            }
        }
    }
}

async fn get_players(
    dbus_conn: &zbus::Connection,
    filter_name: Option<&str>,
    exclude_regex: &[Regex],
) -> Result<Vec<Player>> {
    let proxy = DBusProxy::new(dbus_conn)
        .await
        .error("failed to create DBusProxy")?;
    let names = proxy
        .list_names()
        .await
        .error("failed to list dbus names")?;
    let mut players = Vec::new();
    for name in names {
        if player_matches(name.as_str(), filter_name, exclude_regex) {
            let owner = proxy
                .get_name_owner(name.as_ref())
                .await
                .unwrap()
                .to_string();
            players.push(Player::new(dbus_conn, name, owner).await?);
        }
    }
    Ok(players)
}

#[derive(Debug)]
struct Player {
    status: Option<PlaybackStatus>,
    owner: String,
    bus_name: OwnedBusName,
    player_proxy: zbus_mpris::PlayerProxy<'static>,
    title: Option<String>,
    artist: Option<String>,
    url: Option<String>,
}

impl Player {
    async fn new(
        dbus_conn: &zbus::Connection,
        bus_name: OwnedBusName,
        owner: String,
    ) -> Result<Player> {
        let proxy = zbus_mpris::PlayerProxy::builder(dbus_conn)
            .destination(bus_name.clone())
            .error("failed to set proxy destination")?
            .build()
            .await
            .error("failed to open player proxy")?;
        let metadata = proxy
            .metadata()
            .await
            .error("failed to obtain player metadata")?;
        let status = proxy
            .playback_status()
            .await
            .error("failed to obtain player status")?;

        Ok(Self {
            status: PlaybackStatus::from_str(&status),
            owner,
            bus_name,
            player_proxy: proxy,
            title: metadata.title().map(Into::into),
            artist: metadata.artist().map(Into::into),
            url: metadata.url().map(Into::into),
        })
    }

    fn update_metadata(&mut self, metadata: zbus_mpris::PlayerMetadata) {
        self.title = metadata.title().map(Into::into);
        self.artist = metadata.artist().map(Into::into);
        self.url = metadata.url().map(Into::into);
    }

    async fn play_pause(&self) -> Result<()> {
        self.player_proxy
            .play_pause()
            .await
            .error("play_pause() failed")
    }

    async fn prev(&self) -> Result<()> {
        self.player_proxy.previous().await.error("prev() failed")
    }

    async fn next(&self) -> Result<()> {
        self.player_proxy.next().await.error("next() failed")
    }

    async fn seek(&self, offset: i64) -> Result<()> {
        match self.player_proxy.seek(offset).await {
            Err(zbus::Error::MethodError(e, _, _))
                if e == "org.freedesktop.DBus.Error.NotSupported" =>
            {
                // TODO show this error somehow
                Ok(())
            }
            other => dbg!(other).error("seek() failed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

impl PlaybackStatus {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "Paused" => Some(Self::Paused),
            "Playing" => Some(Self::Playing),
            "Stopped" => Some(Self::Stopped),
            _ => None,
        }
    }
}

fn extract_player_name(full_name: &str) -> Option<&str> {
    const NAME_PREFIX: &str = "org.mpris.MediaPlayer2.";
    full_name
        .starts_with(NAME_PREFIX)
        .then(|| &full_name[NAME_PREFIX.len()..])
}

fn player_matches(full_name: &str, filter_name: Option<&str>, exclude_regex: &[Regex]) -> bool {
    let name = match extract_player_name(full_name) {
        Some(name) => name,
        None => return false,
    };

    filter_name.map_or(true, |f| name.starts_with(f))
        && !exclude_regex.iter().any(|r| r.is_match(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_player_name_test() {
        assert_eq!(
            extract_player_name("org.mpris.MediaPlayer2.firefox.instance852"),
            Some("firefox.instance852")
        );
        assert_eq!(
            extract_player_name("not.org.mpris.MediaPlayer2.firefox.instance852"),
            None,
        );
        assert_eq!(
            extract_player_name("org.mpris.MediaPlayer3.firefox.instance852"),
            None,
        );
    }

    #[test]
    fn player_matches_test() {
        let exclude = vec![Regex::new("mpd").unwrap(), Regex::new("firefox.*").unwrap()];
        assert!(player_matches(
            "org.mpris.MediaPlayer2.playerctld",
            None,
            &exclude
        ));
        assert!(!player_matches(
            "org.mpris.MediaPlayer2.playerctld",
            Some("spotify"),
            &exclude
        ));
        assert!(!player_matches(
            "org.mpris.MediaPlayer2.firefox.instance852",
            None,
            &exclude
        ));
    }
}
