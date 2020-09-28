use std::net::TcpStream;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use uuid::Uuid;
use mpd::Client;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;
use crate::util::FormatTemplate;
use std::collections::hash_map::RandomState;
use std::collections::HashMap;

pub struct Mpd {
    text: TextWidget,
    id: String,
    update_interval: Duration,
    mpd_conn: Client<TcpStream>,
    format: FormatTemplate,

    //useful, but optional
    #[allow(dead_code)]
    config: Config,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct MpdConfig {
    /// Update interval in seconds
    #[serde(
    default = "MpdConfig::default_interval",
    deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    #[serde(default = "MpdConfig::default_format")]
    pub format: String,

    #[serde(default = "MpdConfig::default_ip")]
    pub ip: String,
}

impl MpdConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(1)
    }
    fn default_format() -> String {
        String::from("{artist} - {title} [{elapsed}/{length}]{repeat}{random}{single}{consume}")
    }

    fn default_ip() -> String {
        String::from("127.0.0.1:6600")
    }
}

impl ConfigBlock for Mpd {
    type Config = MpdConfig;
    fn new(
        block_config: Self::Config,
        config: Config,
        tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(Mpd {
            id: Uuid::new_v4().to_simple().to_string(),
            update_interval: block_config.interval,
            text: TextWidget::new(config.clone()).with_text("Mpd"),
            mpd_conn: Client::connect(&block_config.ip).unwrap(),
            format: FormatTemplate::from_string(&block_config.format)
                .block_error(
                    "mpd",
                    "Invalid format for mpd format",
                )?,
            tx_update_request,
            config,
        })
    }
}

impl Block for Mpd {
    fn update(&mut self) -> Result<Option<Update>> {
        let status = self.mpd_conn.status().unwrap();
        let repeat = if status.repeat {"R"} else {""}; //R
        let random = if status.random {"Z"} else {""}; //Z
        let consume = if status.consume {"C"} else {""}; //C
        let single = if status.single {"S"} else {""};

        let title: String = match self.mpd_conn.currentsong().unwrap() {
            Some(song) => {
                match song.title {
                    Some(title) => title,
                    None => song.file
                }
            }
            _ => { String::new() }
        };
        let artist: String = match self.mpd_conn.currentsong().unwrap() {
            Some(song) => {
                match song.tags.get("Artist") {
                    Some(artist) => format!("{}", artist),
                    None => String::from("unknown artist")
                }
            }
            _ => { String::new() }
        };
        let elapsed: String = match status.elapsed {
            Some(te) => format!("{}:{:02}", te.num_seconds()/60, te.num_seconds()%60),
            _ => { String::new() }
        };
        let length: String = match self.mpd_conn.currentsong().unwrap() {
            Some(song) => {
                match song.duration {
                    Some(sl) => format!("{}:{:02}", sl.num_seconds()/60, sl.num_seconds()%60),
                    _ => { String::new() }
                }
            }
            _ => { String::new() }
        };

        let format_values: HashMap<&str, &str, RandomState> = map!("{repeat}" => repeat,
                                                    "{random}" => random,
                                                    "{single}" => single,
                                                    "{consume}" => consume,
                                                    "{artist}" => &artist,
                                                    "{title}" => &title,
                                                    "{elapsed}" => &elapsed,
                                                    "{length}" => &length);

        self.text.set_text(self.format.render_static_str(&format_values)?);
        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}

