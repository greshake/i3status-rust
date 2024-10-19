//! Calendar
//!
//! This block displays upcoming calendar events retrieved from a CalDav ICalendar server.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `next_event_format` | A string to customize the output of this block when there is a next event in the calendar. See below for available placeholders. | <code>\" $icon $start.datetime(f:'%a %H:%M') $summary \"</code>
//! `ongoing_event_format` | A string to customize the output of this block when an event is ongoing. | <code>\" $icon $summary (ends at $end.datetime(f:'%H:%M')) \"</code>
//! `no_events_format` | A string to customize the output of this block when there are no events | <code>\" $icon \"</code>
//! `redirect_format` | A string to customize the output of this block when the authorization is asked | <code>\" $icon Check your web browser \"</code>
//! `fetch_interval` | Fetch events interval in seconds | `60`
//! `alternate_events_interval` | Alternate overlapping events interval in seconds | `10`
//! `events_within_hours` | Number of hours to look for events in the future | `48`
//! `source` | Array of sources to pull calendars from | `[]`
//! `warning_threshold` | Warning threshold in seconds for the upcoming event | `300`
//! `browser_cmd` | Command to open event details in a browser. The block passes the HTML link as an argument | `"xdg-open"`
//!
//! # Source Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `url` | CalDav calendar server URL | N/A
//! `auth` | Authentication configuration (unauthenticated, basic, or oauth2) | `unauthenticated`
//! `calendars` | List of calendar names to monitor. If empty, all calendars will be fetched. | `[]`
//!
//! Note: Currently only one source is supported
//!
//! Action          | Description                               | Default button
//! ----------------|-------------------------------------------|---------------
//! `open_link` | Opens the HTML link of the event | Left
//!
//! # Examples
//!
//! ## Unauthenticated
//!
//! ```toml
//! [[block]]
//! block = "calendar"
//! next_event_format = " $icon $start.datetime(f:'%a %H:%M') $summary "
//! ongoing_event_format = " $icon $summary (ends at $end.datetime(f:'%H:%M')) "
//! no_events_format = " $icon no events "
//! fetch_interval = 30
//! alternate_events_interval = 10
//! events_within_hours = 48
//! warning_threshold = 600
//! browser_cmd = "firefox"
//! [[block.source]]
//! url = "https://caldav.example.com/calendar/"
//! calendars = ["user/calendar"]
//! [block.source.auth]
//! type = "unauthenticated"
//! ```
//!
//! ## Basic Authentication
//!
//! ```toml
//! [[block]]
//! block = "calendar"
//! next_event_format = " $icon $start.datetime(f:'%a %H:%M') $summary "
//! ongoing_event_format = " $icon $summary (ends at $end.datetime(f:'%H:%M')) "
//! no_events_format = " $icon no events "
//! fetch_interval = 30
//! alternate_events_interval = 10
//! events_within_hours = 48
//! warning_threshold = 600
//! browser_cmd = "firefox"
//! [[block.source]]
//! url = "https://caldav.example.com/calendar/"
//! calendars = [ "Holidays" ]
//! [block.source.auth]
//! type = "basic"
//! username = "your_username"
//! password = "your_password"
//! ```
//!
//! Note: You can also configure the `username` and `password` in a separate TOML file.
//!
//! `~/.config/i3status-rust/example_credentials.toml`
//! ```toml
//! username = "my-username"
//! password = "my-password"
//! ```
//!
//! Source auth configuration with `credentials_path`:
//!
//! ```toml
//! [block.source.auth]
//! type = "basic"
//! credentials_path = "~/.config/i3status-rust/example_credentials.toml"
//! ```
//!
//! ## OAuth2 Authentication (Google Calendar)
//!
//! To access the CalDav API of Google, follow these steps to enable the API and obtain the `client_id` and `client_secret`:
//! 1. **Go to the Google Cloud Console**: Navigate to the [Google Cloud Console](https://console.cloud.google.com/).
//! 2. **Create a New Project**: If you don't already have a project, click on the project dropdown and select "New Project". Give your project a name and click "Create".
//! 3. **Enable the CalDAV API**: In the project dashboard, go to the "APIs & Services" > "Library". Search for "CalDAV API" and click on it, then click "Enable".
//! 4. **Set Up OAuth Consent Screen**: Go to "APIs & Services" > "OAuth consent screen". Fill out the required information and save.
//! 5. **Create Credentials**:
//!    - Navigate to "APIs & Services" > "Credentials".
//!    - Click "Create Credentials" and select "OAuth 2.0 Client IDs".
//!    - Configure the consent screen if you haven't already.
//!    - Set the application type to "Web application".
//!    - Add your authorized redirect URIs. For example, `http://localhost:8080`.
//!    - Click "Create" and note down the `client_id` and `client_secret`.
//! 6. **Download the Credentials**: Click on the download icon next to your OAuth 2.0 Client ID to download the JSON file containing your client ID and client secret. Use these values in your configuration.
//!
//! ```toml
//! [[block]]
//! block = "calendar"
//! next_event_format = " $icon $start.datetime(f:'%a %H:%M') $summary "
//! ongoing_event_format = " $icon $summary (ends at $end.datetime(f:'%H:%M')) "
//! no_events_format = " $icon no events "
//! fetch_interval = 30
//! alternate_events_interval = 10
//! events_within_hours = 48
//! warning_threshold = 600
//! browser_cmd = "firefox"
//! [[block.source]]
//! url = "https://apidata.googleusercontent.com/caldav/v2/"
//! calendars = ["primary"]
//! [block.source.auth]
//! type = "oauth2"
//! client_id = "your_client_id"
//! client_secret = "your_client_secret"
//! auth_url = "https://accounts.google.com/o/oauth2/auth"
//! token_url = "https://oauth2.googleapis.com/token"
//! auth_token = "~/.config/i3status-rust/calendar.auth_token"
//! redirect_port = 8080
//! scopes = ["https://www.googleapis.com/auth/calendar", "https://www.googleapis.com/auth/calendar.events"]
//! ```
//!
//! Note: You can also configure the `client_id` and `client_secret` in a separate TOML file.
//!
//! `~/.config/i3status-rust/google_credentials.toml`
//! ```toml
//! client_id = "my-client_id"
//! client_secret = "my-client_secret"
//! ```
//!
//! Source auth configuration with `credentials_path`:
//!
//! ```toml
//! [block.source.auth]
//! type = "oauth2"
//! credentials_path = "~/.config/i3status-rust/google_credentials.toml"
//! auth_url = "https://accounts.google.com/o/oauth2/auth"
//! token_url = "https://oauth2.googleapis.com/token"
//! auth_token = "~/.config/i3status-rust/calendar.auth_token"
//! redirect_port = 8080
//! scopes = ["https://www.googleapis.com/auth/calendar", "https://www.googleapis.com/auth/calendar.events"]
//! ```
//!
//! # Format Configuration
//!
//! The format configuration is a string that can include placeholders to be replaced with dynamic content.
//! Placeholders can be:
//! - `$summary`: Summary of the event
//! - `$description`: Description of the event
//! - `$url`: Url of the event
//! - `$location`: Location of the event
//! - `$start`: Start time of the event
//! - `$end`: End time of the event
//!
//! # Icons Used
//! - `calendar`

use chrono::{Duration, Local, Utc};
use oauth2::{AuthUrl, ClientId, ClientSecret, Scope, TokenUrl};
use reqwest::Url;

use crate::util;
use crate::{subprocess::spawn_process, util::has_command};

mod auth;
mod caldav;

use self::auth::{Authorize, AuthorizeUrl, OAuth2Flow, TokenStore, TokenStoreError};
use self::caldav::Event;

use super::prelude::*;

use std::path::Path;
use std::sync::Arc;

use caldav::Client;

#[derive(Deserialize, Debug, SmartDefault, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct BasicCredentials {
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BasicAuthConfig {
    #[serde(flatten)]
    pub credentials: BasicCredentials,
    pub credentials_path: Option<ShellString>,
}

#[derive(Deserialize, Debug, SmartDefault, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct OAuth2Credentials {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

#[derive(Deserialize, Debug, SmartDefault, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct OAuth2Config {
    #[serde(flatten)]
    pub credentials: OAuth2Credentials,
    pub credentials_path: Option<ShellString>,
    pub auth_url: String,
    pub token_url: String,
    #[default("~/.config/i3status-rust/calendar.auth_token".into())]
    pub auth_token: ShellString,
    #[default(8080)]
    pub redirect_port: u16,
    pub scopes: Vec<Scope>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthConfig {
    #[default]
    Unauthenticated,
    Basic(BasicAuthConfig),
    OAuth2(OAuth2Config),
}

#[derive(Deserialize, Debug, SmartDefault, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct SourceConfig {
    pub url: String,
    pub auth: AuthConfig,
    pub calendars: Vec<String>,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub next_event_format: FormatConfig,
    pub ongoing_event_format: FormatConfig,
    pub no_events_format: FormatConfig,
    pub redirect_format: FormatConfig,
    #[default(60.into())]
    pub fetch_interval: Seconds,
    #[default(10.into())]
    pub alternate_events_interval: Seconds,
    #[default(48)]
    pub events_within_hours: u32,
    pub source: Vec<SourceConfig>,
    #[default(300)]
    pub warning_threshold: u32,
    #[default("xdg-open".into())]
    pub browser_cmd: ShellString,
}

enum WidgetStatus {
    AlternateEvents,
    FetchSources,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let next_event_format = config
        .next_event_format
        .with_default(" $icon $start.datetime(f:'%a %H:%M') $summary ")?;
    let ongoing_event_format = config
        .ongoing_event_format
        .with_default(" $icon $summary (ends at $end.datetime(f:'%H:%M')) ")?;
    let no_events_format = config.no_events_format.with_default(" $icon ")?;
    let redirect_format = config
        .redirect_format
        .with_default(" $icon Check your web browser ")?;

    api.set_default_actions(&[(MouseButton::Left, None, "open_link")])?;

    let source_config = match config.source.len() {
        0 => return Err(Error::new("A calendar source must be supplied")),
        1 => config
            .source
            .first()
            .expect("There must be a first entry since the length is 1"),
        _ => {
            return Err(Error::new(
                "Currently only one calendar source is supported",
            ))
        }
    };

    let warning_threshold = Duration::try_seconds(config.warning_threshold.into())
        .error("Invalid warning threshold configuration")?;

    let mut source = Source::new(source_config.clone()).await?;

    let mut timer = config.fetch_interval.timer();

    let mut alternate_events_timer = config.alternate_events_interval.timer();

    let mut actions = api.get_actions()?;

    let events_within = Duration::try_hours(config.events_within_hours.into())
        .error("Invalid events within hours configuration")?;

    let mut widget_status = WidgetStatus::FetchSources;

    let mut next_events = OverlappingEvents::default();

    loop {
        let mut widget = Widget::new().with_format(no_events_format.clone());
        widget.set_values(map! {
            "icon" => Value::icon("calendar"),
        });

        if matches!(widget_status, WidgetStatus::FetchSources) {
            for retries in 0..=1 {
                match source.get_next_events(events_within).await {
                    Ok(events) => {
                        next_events.refresh(events);
                        break;
                    }
                    Err(err) => match err {
                        CalendarError::AuthRequired => {
                            let authorization = source
                                .client
                                .authorize()
                                .await
                                .error("Authorization failed")?;
                            match &authorization {
                                Authorize::AskUser(AuthorizeUrl { url, .. }) if retries == 0 => {
                                    widget.set_format(redirect_format.clone());
                                    api.set_widget(widget.clone())?;
                                    open_browser(config, url).await?;
                                    source
                                        .client
                                        .ask_user(authorization)
                                        .await
                                        .error("Ask user failed")?;
                                }
                                _ => {
                                    return Err(Error::new(
                                        "Authorization failed. Check your configurations",
                                    ))
                                }
                            }
                        }
                        e => {
                            return Err(Error {
                                message: None,
                                cause: Some(Arc::new(e)),
                            })
                        }
                    },
                };
            }
        }

        if let Some(event) = next_events.current().cloned() {
            if let (Some(start_date), Some(end_date)) = (event.start_at, event.end_at) {
                let warn_datetime = start_date - warning_threshold;
                if warn_datetime < Utc::now() && Utc::now() < start_date {
                    widget.state = State::Warning;
                }
                if start_date < Utc::now() && Utc::now() < end_date {
                    widget.set_format(ongoing_event_format.clone());
                } else {
                    widget.set_format(next_event_format.clone());
                }
                widget.set_values(map! {
                  "icon" => Value::icon("calendar"),
                   [if let Some(summary) = event.summary] "summary" => Value::text(summary),
                   [if let Some(description) = event.description] "description" => Value::text(description),
                   [if let Some(location) = event.location] "location" => Value::text(location),
                   [if let Some(url) = event.url] "url" => Value::text(url),
                   "start" => Value::datetime(start_date, None),
                   "end" => Value::datetime(end_date, None),
                });
            }
        }

        api.set_widget(widget)?;
        loop {
            select! {
                _ = timer.tick() => {
                  widget_status = WidgetStatus::FetchSources;
                  break
                }
                _ = alternate_events_timer.tick() => {
                  next_events.cycle_warning_or_ongoing(warning_threshold);
                  widget_status = WidgetStatus::AlternateEvents;
                  break
                }
                _ = api.wait_for_update_request() => break,
                Some(action) = actions.recv() => match action.as_ref() {
                      "open_link" => {
                          if let Some(Event { url: Some(url), .. }) = next_events.current(){
                              if let Ok(url) = Url::parse(url) {
                                  open_browser(config, &url).await?;
                              }
                          }
                      }
                      _ => ()
                }
            }
        }
    }
}

struct Source {
    pub client: caldav::Client,
    pub config: SourceConfig,
}

impl Source {
    async fn new(config: SourceConfig) -> Result<Self> {
        let auth = match &config.auth {
            AuthConfig::Unauthenticated => auth::Auth::Unauthenticated,
            AuthConfig::Basic(BasicAuthConfig {
                credentials,
                credentials_path,
            }) => {
                let credentials = if let Some(path) = credentials_path {
                    util::deserialize_toml_file(path.expand()?.to_string())
                        .error("Failed to read basic credentials file")?
                } else {
                    credentials.clone()
                };
                let BasicCredentials {
                    username: Some(username),
                    password: Some(password),
                } = credentials
                else {
                    return Err(Error::new("Basic credentials are not configured"));
                };
                auth::Auth::basic(username, password)
            }
            AuthConfig::OAuth2(oauth2) => {
                let credentials = if let Some(path) = &oauth2.credentials_path {
                    util::deserialize_toml_file(path.expand()?.to_string())
                        .error("Failed to read oauth2 credentials file")?
                } else {
                    oauth2.credentials.clone()
                };
                let OAuth2Credentials {
                    client_id: Some(client_id),
                    client_secret: Some(client_secret),
                } = credentials
                else {
                    return Err(Error::new("Oauth2 credentials are not configured"));
                };
                let auth_url =
                    AuthUrl::new(oauth2.auth_url.clone()).error("Invalid authorization url")?;
                let token_url =
                    TokenUrl::new(oauth2.token_url.clone()).error("Invalid token url")?;

                let flow = OAuth2Flow::new(
                    ClientId::new(client_id),
                    ClientSecret::new(client_secret),
                    auth_url,
                    token_url,
                    oauth2.redirect_port,
                );
                let token_store =
                    TokenStore::new(Path::new(&oauth2.auth_token.expand()?.to_string()));
                auth::Auth::oauth2(flow, token_store, oauth2.scopes.clone())
            }
        };
        Ok(Self {
            client: Client::new(
                Url::parse(&config.url).error("Invalid CalDav server url")?,
                auth,
            ),
            config,
        })
    }

    async fn get_next_events(
        &mut self,
        within: Duration,
    ) -> Result<OverlappingEvents, CalendarError> {
        let calendars: Vec<_> = self
            .client
            .calendars()
            .await?
            .into_iter()
            .filter(|c| self.config.calendars.is_empty() || self.config.calendars.contains(&c.name))
            .collect();
        let mut events: Vec<Event> = vec![];
        for calendar in calendars {
            let calendar_events: Vec<_> = self
                .client
                .events(
                    &calendar,
                    Local::now()
                        .date_naive()
                        .and_hms_opt(0, 0, 0)
                        .expect("A valid time")
                        .and_local_timezone(Local)
                        .earliest()
                        .expect("A valid datetime")
                        .to_utc(),
                    Utc::now() + within,
                )
                .await?
                .into_iter()
                .filter(|e| {
                    let not_started = e.start_at.is_some_and(|d| d > Utc::now());
                    let is_ongoing = e.start_at.is_some_and(|d| d < Utc::now())
                        && e.end_at.is_some_and(|d| d > Utc::now());
                    not_started || is_ongoing
                })
                .collect();
            events.extend(calendar_events);
        }

        events.sort_by_key(|e| e.start_at);
        let Some(next_event) = events.first().cloned() else {
            return Ok(OverlappingEvents::default());
        };
        let overlapping_events = events
            .into_iter()
            .take_while(|e| e.start_at <= next_event.end_at)
            .collect();
        Ok(OverlappingEvents::new(overlapping_events))
    }
}

#[derive(Default)]
struct OverlappingEvents {
    current: Option<Event>,
    events: Vec<Event>,
}

impl OverlappingEvents {
    fn new(events: Vec<Event>) -> Self {
        Self {
            current: events.first().cloned(),
            events,
        }
    }

    fn refresh(&mut self, other: OverlappingEvents) {
        if self.current.is_none() {
            self.current = other.events.first().cloned();
        }
        self.events = other.events;
    }

    fn current(&self) -> Option<&Event> {
        self.current.as_ref()
    }

    fn cycle_warning_or_ongoing(&mut self, warning_threshold: Duration) {
        self.current = if let Some(current) = &self.current {
            if self.events.iter().any(|e| e.uid == current.uid) {
                let mut iter = self
                    .events
                    .iter()
                    .cycle()
                    .skip_while(|e| e.uid != current.uid);
                iter.next();
                iter.find(|e| {
                    let is_ongoing = e.start_at.is_some_and(|d| d < Utc::now())
                        && e.end_at.is_some_and(|d| d > Utc::now());
                    let is_warning = e
                        .start_at
                        .is_some_and(|d| d - warning_threshold < Utc::now() && Utc::now() < d);
                    e.uid == current.uid || is_warning || is_ongoing
                })
                .cloned()
            } else {
                self.events.first().cloned()
            }
        } else {
            self.events.first().cloned()
        };
    }
}

async fn open_browser(config: &Config, url: &Url) -> Result<()> {
    let cmd = config.browser_cmd.expand()?;
    has_command(&cmd)
        .await
        .or_error(|| "Browser command not found")?;
    spawn_process(&cmd, &[url.as_ref()]).error("Open browser failed")
}

#[derive(thiserror::Error, Debug)]
pub enum CalendarError {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Deserialize(#[from] quick_xml::de::DeError),
    #[error("Parsing error: {0}")]
    Parsing(String),
    #[error("Auth required")]
    AuthRequired,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serialize(#[from] serde_json::Error),
    #[error("Request token error: {0}")]
    RequestToken(String),
    #[error("Store token error: {0}")]
    StoreToken(#[from] TokenStoreError),
}
