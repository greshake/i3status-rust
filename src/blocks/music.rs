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
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>" $icon {$combo.rot-str() $play &vert;}"</code>
//! `player` | Name(s) of the music player(s) MPRIS interface. This can be either a music player name or an array of music player names. Run <code>busctl --user list &vert; grep "org.mpris.MediaPlayer2." &vert; cut -d' ' -f1</code> and the name is the part after "org.mpris.MediaPlayer2.". | `None`
//! `interface_name_exclude` | A list of regex patterns for player MPRIS interface names to ignore. | `[]`
//! `separator` | String to insert between artist and title. | `" - "`
//! `seek_step` | Number of microseconds to seek forward/backward when scrolling on the bar. | `1000`
//!
//! Note: All placeholders exctpt `icon` can be absent. See the examples below to learn how to handle this.
//!
//! Placeholder | Value          | Type
//! ------------|----------------|------
//! `icon`      | A static icon  | Icon
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
//! Action          | Default button
//! ----------------|------------------
//! `play_pause`    | Left on `$play`
//! `next`          | Left on `$next`
//! `prev`          | Left on `$prev`
//! `next_player`   | Right
//! `seek_forward`  | Wheel Up
//! `seek_backward` | Wheel Down
//!
//! # Examples
//!
//! Show the currently playing song on Spotify only, with play & next buttons and limit the width
//! to 20 characters:
//!
//! ```toml
//! [[block]]
//! block = "music"
//! format = " $icon {$combo.str(max_w:20) $play $next |}"
//! player = "spotify"
//! ```
//!
//! Same thing for any compatible player, takes the first active on the bus, but ignores "mpd" or anything with "kdeconnect" in the name:
//!
//! ```toml
//! [[block]]
//! block = "music"
//! format = " $icon {$combo.str(max_w:20) $play $next |}"
//! interface_name_exclude = [".*kdeconnect.*", "mpd"]
//! ```
//!
//! Same as above, but displays with rotating text
//!
//! ```toml
//! [[block]]
//! block = "music"
//! format = " $icon {$combo.rot-str(w:20) $play $next |}"
//! interface_name_exclude = [".*kdeconnect.*", "mpd"]
//! ```
//!
//! Click anywhere to paly/pause:
//!
//! ```toml
//! [[block]]
//! block = "music"
//! [[block.click]]
//! button = "left"
//! action = "play_pause"
//! ```
//!
//! # Icons Used
//! - `music`
//! - `music_next`
//! - `music_play`
//! - `music_prev`
//!
//! [MediaPlayer2 Interface]: https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html

use super::prelude::*;
use regex::Regex;
use zbus::fdo::{DBusProxy, NameOwnerChanged, PropertiesChanged};
use zbus::names::{OwnedBusName, OwnedUniqueName};
use zbus::{MatchRule, MessageStream};

mod zbus_mpris;

make_log_macro!(debug, "music");

const PLAY_PAUSE_BTN: &str = "play_pause_btn";
const NEXT_BTN: &str = "next_btn";
const PREV_BTN: &str = "prev_btn";

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    format: FormatConfig,
    player: PlayerName,
    interface_name_exclude: Vec<String>,
    #[default(" - ".into())]
    separator: String,
    #[default(1_000)]
    seek_step: i64,
}

#[derive(Deserialize, Debug, Clone, SmartDefault)]
#[serde(untagged)]
pub enum PlayerName {
    Single(String),
    #[default]
    Multiple(Vec<String>),
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[
        (MouseButton::Left, Some(PLAY_PAUSE_BTN), "play_pause"),
        (MouseButton::Left, Some(NEXT_BTN), "next"),
        (MouseButton::Left, Some(PREV_BTN), "prev"),
        (MouseButton::Right, None, "next_player"),
        (MouseButton::WheelUp, None, "seek_forward"),
        (MouseButton::WheelDown, None, "seek_backward"),
    ])
    .await?;

    let dbus_conn = new_dbus_connection().await?;
    let mut widget = Widget::new().with_format(
        config
            .format
            .with_default(" $icon {$combo.rot-str() $play |}")?,
    );

    let new_btn = |icon: &str, instance: &'static str, api: &mut CommonApi| -> Result<Value> {
        Ok(Value::icon(api.get_icon(icon)?).with_instance(instance))
    };

    let values = map! {
        "icon" => Value::icon(api.get_icon("music")?),
        "next" => new_btn("music_next", NEXT_BTN, &mut api)?,
        "prev" => new_btn("music_prev", PREV_BTN, &mut api)?,
    };

    let prefered_players = match config.player {
        PlayerName::Single(name) => vec![name],
        PlayerName::Multiple(names) => names,
    };
    let exclude_regex = config
        .interface_name_exclude
        .iter()
        .map(|r| Regex::new(r))
        .collect::<Result<Vec<_>, _>>()
        .error("Invalid regex")?;

    let mut players = get_players(&dbus_conn, &prefered_players, &exclude_regex).await?;
    let mut cur_player = None;
    for (i, player) in players.iter().enumerate() {
        cur_player = Some(i);
        if player.status == Some(PlaybackStatus::Playing) {
            break;
        }
    }

    let mut properties_stream = MessageStream::for_match_rule(
        MatchRule::builder()
            .msg_type(zbus::MessageType::Signal)
            .interface("org.freedesktop.DBus.Properties")
            .and_then(|x| x.member("PropertiesChanged"))
            .and_then(|x| x.path("/org/mpris/MediaPlayer2"))
            .unwrap()
            .build(),
        &dbus_conn,
        None,
    )
    .await
    .error("Failed to add match rule")?;

    let mut name_owner_changed_stream = MessageStream::for_match_rule(
        MatchRule::builder()
            .msg_type(zbus::MessageType::Signal)
            .interface("org.freedesktop.DBus")
            .and_then(|x| x.member("NameOwnerChanged"))
            .and_then(|x| x.arg0namespace("org.mpris.MediaPlayer2"))
            .unwrap()
            .build(),
        &dbus_conn,
        None,
    )
    .await
    .error("Failed to add match rule")?;

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
                if let Some(url) = &player.metadata.url {
                    values.insert("url".into(), Value::text(url.clone()));
                }
                match (
                    &player.metadata.title,
                    &player.metadata.artist,
                    &player.metadata.url,
                ) {
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
            None => {
                widget.set_values(map!("icon" => Value::icon(api.get_icon("music")?)));
                widget.state = State::Idle;
                api.set_widget(&widget).await?;
            }
        }

        loop {
            select! {
                Some(msg) = properties_stream.next() => {
                    let msg = msg.unwrap();
                    let msg = PropertiesChanged::from_message(msg).unwrap();
                    let args = msg.args().unwrap();
                    let header = msg.header().unwrap();
                    let sender = header.sender().unwrap().unwrap();
                    if let Some(player) = players.iter_mut().find(|p| &*p.owner == sender) {
                        let props = args.changed_properties;
                        if let Some(status) = props.get("PlaybackStatus") {
                            let status: &str = status.downcast_ref().unwrap();
                            player.status = PlaybackStatus::from_str(status);
                        }
                        if let Some(metadata) = props.get("Metadata") {
                            player.metadata =
                                zbus_mpris::PlayerMetadata::try_from(metadata.to_owned()).unwrap();
                        }
                        break;
                    }
                }
                Some(msg) = name_owner_changed_stream.next() => {
                    let msg = msg.unwrap();
                    let msg = NameOwnerChanged::from_message(msg).unwrap();
                    let args = msg.args().unwrap();
                    match (args.old_owner.as_ref(), args.new_owner.as_ref()) {
                        (None, Some(new)) => if player_matches(args.name.as_str(), &prefered_players, &exclude_regex) {
                            players.push(Player::new(&dbus_conn, args.name.to_owned().into(), new.to_owned().into()).await?);
                            cur_player = Some(players.len() - 1);
                        }
                        (Some(old), None) => {
                            if let Some(pos) = players.iter().position(|p| &*p.owner == old) {
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
                    break;
                }
                event = api.event() => match event {
                    UpdateRequest => (),
                    Action(a) => {
                        if let Some(i) = cur_player {
                            let player = &players[i];
                            match a.as_ref() {
                                "play_pause" => {
                                    player.play_pause().await?;
                                }
                                "next" => {
                                    player.next().await?;
                                }
                                "prev" => {
                                    player.prev().await?;
                                }
                                "next_player" => {
                                    if i + 1 < players.len() {
                                        cur_player = Some(i + 1);
                                    } else {
                                        cur_player = Some(0);
                                    }
                                }
                                "seek_forward" => {
                                    player.seek(config.seek_step).await?;
                                }
                                "seek_backward" => {
                                    player.seek(-config.seek_step).await?;
                                }
                                _ => (),
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn get_players(
    dbus_conn: &zbus::Connection,
    prefered_players: &[String],
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
        if player_matches(name.as_str(), prefered_players, exclude_regex) {
            let owner = proxy.get_name_owner(name.as_ref()).await.unwrap();
            players.push(Player::new(dbus_conn, name, owner).await?);
        }
    }
    Ok(players)
}

#[derive(Debug)]
struct Player {
    status: Option<PlaybackStatus>,
    owner: OwnedUniqueName,
    bus_name: OwnedBusName,
    player_proxy: zbus_mpris::PlayerProxy<'static>,
    metadata: zbus_mpris::PlayerMetadata,
}

impl Player {
    async fn new(
        dbus_conn: &zbus::Connection,
        bus_name: OwnedBusName,
        owner: OwnedUniqueName,
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
            metadata,
        })
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

fn player_matches(full_name: &str, prefered_players: &[String], exclude_regex: &[Regex]) -> bool {
    let name = match extract_player_name(full_name) {
        Some(name) => name,
        None => return false,
    };

    exclude_regex.iter().all(|r| !r.is_match(name))
        && (prefered_players.is_empty() || prefered_players.iter().any(|p| name.starts_with(&**p)))
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
            &[],
            &exclude
        ));
        assert!(!player_matches(
            "org.mpris.MediaPlayer2.playerctld",
            &["spotify".into()],
            &exclude
        ));
        assert!(!player_matches(
            "org.mpris.MediaPlayer2.firefox.instance852",
            &[],
            &exclude
        ));
    }
}
