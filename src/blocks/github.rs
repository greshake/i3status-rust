use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use crossbeam_channel::Sender;
use lazy_static::lazy_static;
use regex::Regex;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::http;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::util::FormatTemplate;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

const GITHUB_TOKEN_ENV: &str = "I3RS_GITHUB_TOKEN";

pub struct Github {
    text: TextWidget,
    id: usize,
    update_interval: Duration,
    api_server: String,
    token: String,
    format: FormatTemplate,
    total_notifications: u64,
    hide_if_total_is_zero: bool,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct GithubConfig {
    /// Update interval in seconds
    #[serde(
        default = "GithubConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    #[serde(default = "GithubConfig::default_api_server")]
    pub api_server: String,

    /// Format override
    #[serde(default = "GithubConfig::default_format")]
    pub format: String,

    #[serde(default = "GithubConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,

    #[serde(default = "GithubConfig::default_hide_if_total_is_zero")]
    pub hide_if_total_is_zero: bool,
}

impl GithubConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(30)
    }

    fn default_api_server() -> String {
        "https://api.github.com".to_owned()
    }

    fn default_format() -> String {
        "{total}".to_owned()
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }

    fn default_hide_if_total_is_zero() -> bool {
        false
    }
}

impl ConfigBlock for Github {
    type Config = GithubConfig;

    fn new(id: usize, block_config: Self::Config, config: Config, _: Sender<Task>) -> Result<Self> {
        let token = std::env::var(GITHUB_TOKEN_ENV)
            .block_error("github", "missing I3RS_GITHUB_TOKEN environment variable")?;

        let text = TextWidget::new(config, id)
            .with_text("x")
            .with_icon("github");
        Ok(Github {
            id,
            update_interval: block_config.interval,
            text,
            api_server: block_config.api_server,
            token,
            format: FormatTemplate::from_string(&block_config.format)
                .block_error("github", "Invalid format specified")?,
            total_notifications: 0,
            hide_if_total_is_zero: block_config.hide_if_total_is_zero,
        })
    }
}

impl Block for Github {
    fn update(&mut self) -> Result<Option<Update>> {
        let aggregations = match Notifications::new(&self.api_server, &self.token).try_fold(
            map!("total".to_owned() => 0),
            |mut acc,
             notif|
             -> std::result::Result<HashMap<String, u64>, Box<dyn std::error::Error>> {
                let n = notif?;
                acc.entry(n.reason).and_modify(|v| *v += 1).or_insert(1);
                acc.entry("total".to_owned()).and_modify(|v| *v += 1);
                Ok(acc)
            },
        ) {
            Ok(v) => v,
            Err(_) => {
                // If there is a error reported, set the value to x
                self.text.set_text("x".to_owned());
                return Ok(Some(self.update_interval.into()));
            }
        };

        let default: u64 = 0;
        self.total_notifications = *aggregations.get("total").unwrap_or(&default);
        let values = map!(
            "{total}" => format!("{}", self.total_notifications),
            // As specified by:
            // https://developer.github.com/v3/activity/notifications/#notification-reasons
            "{assign}" => format!("{}", aggregations.get("assign").unwrap_or(&default)),
            "{author}" => format!("{}", aggregations.get("author").unwrap_or(&default)),
            "{comment}" => format!("{}", aggregations.get("comment").unwrap_or(&default)),
            "{invitation}" => format!("{}", aggregations.get("invitation").unwrap_or(&default)),
            "{manual}" => format!("{}", aggregations.get("manual").unwrap_or(&default)),
            "{mention}" => format!("{}", aggregations.get("mention").unwrap_or(&default)),
            "{review_requested}" => format!("{}", aggregations.get("review_requested").unwrap_or(&default)),
            "{security_alert}" => format!("{}", aggregations.get("security_alert").unwrap_or(&default)),
            "{state_change}" => format!("{}", aggregations.get("state_change").unwrap_or(&default)),
            "{subscribed}" => format!("{}", aggregations.get("subscribed").unwrap_or(&default)),
            "{team_mention}" => format!("{}", aggregations.get("team_mention").unwrap_or(&default))
        );

        self.text.set_text(self.format.render_static_str(&values)?);

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        if self.hide_if_total_is_zero && self.total_notifications == 0 {
            vec![]
        } else {
            vec![&self.text]
        }
    }

    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}

#[derive(Deserialize)]
struct Notification {
    reason: String,
}

struct Notifications<'a> {
    notifications: <Vec<Notification> as IntoIterator>::IntoIter,
    token: &'a str,
    next_page_url: String,
}

impl<'a> Iterator for Notifications<'a> {
    type Item = std::result::Result<Notification, Box<dyn std::error::Error>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.try_next() {
            Ok(Some(notif)) => Some(Ok(notif)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

impl<'a> Notifications<'a> {
    fn new(api_server: &'a str, token: &'a str) -> Notifications<'a> {
        Notifications {
            next_page_url: format!("{}/notifications", api_server),
            token,
            notifications: vec![].into_iter(),
        }
    }

    fn try_next(
        &mut self,
    ) -> std::result::Result<Option<Notification>, Box<dyn std::error::Error>> {
        if let Some(notif) = self.notifications.next() {
            return Ok(Some(notif));
        }

        if self.next_page_url.is_empty() {
            return Ok(None);
        }

        let header_value = format!("Bearer {}", self.token);
        let headers = vec![("Authorization", header_value.as_str())];
        let result =
            http::http_get_json(&self.next_page_url, Some(Duration::from_secs(3)), headers)?;

        self.next_page_url = result
            .headers
            .iter()
            .find_map(|header| {
                if header.starts_with("Link:") {
                    parse_links_header(header).get("next").cloned()
                } else {
                    None
                }
            })
            .unwrap_or(&"")
            .to_string();

        let notifications: Vec<Notification> = serde_json::from_value(result.content)?;
        self.notifications = notifications.into_iter();

        Ok(self.notifications.next())
    }
}

fn parse_links_header(raw_links: &str) -> HashMap<&str, &str> {
    lazy_static! {
        static ref LINKS_REGEX: Regex =
            Regex::new(r#"(<(?P<url>http(s)?://[^>\s]+)>; rel="(?P<rel>[[:word:]]+))+"#).unwrap();
    }

    LINKS_REGEX
        .captures_iter(raw_links)
        .fold(HashMap::new(), |mut acc, cap| {
            let groups = (cap.name("url"), cap.name("rel"));
            match groups {
                (Some(url), Some(rel)) => {
                    acc.insert(rel.as_str(), url.as_str());
                    acc
                }
                _ => acc,
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_parses_links_header() {
        assert_eq!(
            parse_links_header(
                r#"Link: <https://api.github.com/notifications?page=1>; rel="prev", <https://api.github.com/notifications?page=3>; rel="next", <https://api.github.com/notifications?page=4>; rel="last", <https://api.github.com/notifications?page=1>; rel="first""#,
            ),
            map!(
                "first" => "https://api.github.com/notifications?page=1",
                "prev" => "https://api.github.com/notifications?page=1",
                "next" => "https://api.github.com/notifications?page=3",
                "last" => "https://api.github.com/notifications?page=4"
            )
        );
    }
}
