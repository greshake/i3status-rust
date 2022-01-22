use zbus::fdo::DBusProxy;
use zbus::names::{OwnedBusName, OwnedInterfaceName};
use zbus::zvariant::{Optional, OwnedValue, Type};
use zbus::MessageStream;

use std::collections::HashMap;

use super::prelude::*;
mod zbus_mpris;
use crate::util::new_dbus_connection;

const PLAY_PAUSE_BTN: usize = 1;
const NEXT_BTN: usize = 2;
const PREV_BTN: usize = 3;

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields, default)]
struct MusicConfig {
    // TODO add stuff here
    buttons: Vec<String>,

    format: FormatConfig,
}

// impl Default for MusicConfig {
//     fn default() -> Self {
//         Self {
//             buttons: Vec::new(),
//             format: Default::default(),
//         }
//     }
// }

#[derive(Debug, Clone, Type, serde_derive::Deserialize)]
struct PropChange {
    _interface_name: OwnedInterfaceName,
    changed_properties: HashMap<StdString, OwnedValue>,
    _invalidated_properties: Vec<StdString>,
}

#[derive(Debug, Clone, Type, serde_derive::Deserialize)]
struct OwnerChange {
    pub name: OwnedBusName,
    pub old_owner: Optional<StdString>,
    pub new_owner: Optional<StdString>,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let dbus_conn = new_dbus_connection().await?;
    let mut events = api.get_events().await?;
    let config = MusicConfig::deserialize(config).config_error()?;
    api.set_format(config.format.with_default("$title_artist.rot-str()|")?);
    api.set_icon("music")?;

    // Init buttons
    for button in &config.buttons {
        match button.as_str() {
            "play" => api.add_button(PLAY_PAUSE_BTN, "music_play")?,
            "next" => api.add_button(NEXT_BTN, "music_next")?,
            "prev" => api.add_button(PREV_BTN, "music_prev")?,
            x => return Err(Error::new(format!("Unknown button: '{}'", x))),
        }
    }

    let mut players = get_all_players(&dbus_conn).await?;
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
        let player = cur_player.map(|c| players.get_mut(c).unwrap());
        match player {
            Some(ref player) => {
                let mut values = HashMap::new();
                player
                    .title
                    .clone()
                    .map(|t| values.insert("title".into(), Value::text(t)));
                player
                    .artist
                    .clone()
                    .map(|t| values.insert("artist".into(), Value::text(t)));
                match (&player.title, &player.artist) {
                    (Some(x), None) | (None, Some(x)) => {
                        values.insert("title_artist".into(), Value::text(x.clone()));
                    }
                    (Some(t), Some(a)) => {
                        values.insert(
                            "title_artist".into(),
                            Value::text(format!("{}|{}", t, a).into()),
                        );
                    }
                    _ => (),
                }
                if let (Some(mut t), Some(a)) = (player.title.clone(), &player.artist) {
                    t.push('|');
                    t.push_str(a);
                    values.insert("title_artist".into(), Value::text(t));
                }
                api.set_values(values);
                api.show_buttons();

                let (state, play_icon) = match player.status {
                    Some(PlaybackStatus::Playing) => (State::Info, "music_pause"),
                    _ => (State::Idle, "music_play"),
                };
                api.set_state(state);
                api.set_button(PLAY_PAUSE_BTN, play_icon)?;
            }
            None => {
                api.set_values(HashMap::new());
                api.hide_buttons();
                api.set_state(State::Idle);
            }
        }

        api.flush().await?;

        tokio::select! {
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
                        let old: Option<StdString> = body.old_owner.into();
                        let new: Option<StdString> = body.new_owner.into();
                        match (old, new) {
                            (None, Some(new)) => if new != body.name.to_string() {
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
            Some(BlockEvent::Click(click)) = events.recv() => {
                match click.button {
                    MouseButton::Left => {
                        if let Some(ref player) = player {
                            match click.instance {
                                Some(PLAY_PAUSE_BTN) => player.play_pause().await?,
                                Some(NEXT_BTN) => player.next().await?,
                                Some(PREV_BTN) => player.prev().await?,
                                _ => (),
                            }
                        }
                    }
                    MouseButton::WheelUp => {
                        if let Some(cur) = cur_player {
                            if cur > 0 {
                                cur_player = Some(cur - 1);
                            }
                        }
                    }
                    MouseButton::WheelDown => {
                        if let Some(cur) = cur_player {
                            if cur + 1 < players.len() {
                                cur_player = Some(cur + 1);
                            }
                        }
                    }
                    _ => (),
                }
            }
        }
    }
}

async fn get_all_players(dbus_conn: &zbus::Connection) -> Result<Vec<Player<'_>>> {
    let proxy = DBusProxy::new(dbus_conn)
        .await
        .error("failed to create DBusProxy")?;
    let names = proxy
        .list_names()
        .await
        .error("failed to list dbus names")?;

    let mut players = Vec::new();
    for name in names {
        if name.starts_with("org.mpris.MediaPlayer2") {
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
struct Player<'a> {
    status: Option<PlaybackStatus>,
    owner: StdString,
    player_proxy: zbus_mpris::PlayerProxy<'a>,
    title: Option<String>,
    artist: Option<String>,
}

impl<'a> Player<'a> {
    async fn new(
        dbus_conn: &'a zbus::Connection,
        bus_name: OwnedBusName,
        owner: StdString,
    ) -> Result<Player<'a>> {
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
            player_proxy: proxy,
            title: metadata.title().map(Into::into),
            artist: metadata.artist().map(Into::into),
        })
    }

    fn update_metadata(&mut self, metadata: zbus_mpris::PlayerMetadata) {
        self.title = metadata.title().map(Into::into);
        self.artist = metadata.artist().map(Into::into);
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
