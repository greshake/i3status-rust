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
//! `interval` | Update interval in seconds | `60`
//! `url` | CalDav calendar server URL | N/A
//! `auth` | Authentication configuration (unauthenticated, basic, or oauth2) | `unauthenticated`
//! `calendars` | List of calendar names to monitor. If empty, all calendars will be fetched. | `[]`
//! `events_within_hours` | Number of hours to look for events in the future | `24`
//! `warning_threshold` | Warning threshold in seconds for the upcoming event | `300`
//! `browser_cmd` | Command to open event details in a browser. The block passes the HTML link as an argument | `"xdg-open"`
//!
//! Action          | Description                               | Default button
//! ----------------|-------------------------------------------|---------------
//! `open_link` | Opens the HTML link of the event | Left
//!
//!//! # Examples
//!
//! ## Unauthenticated
//!
//! ```toml
//! [[block]]
//! block = "calendar"
//! next_event_format = " $icon $start.datetime(f:'%a %H:%M') $summary "
//! ongoing_event_format = " $icon $summary (ends at $end.datetime(f:'%H:%M')) "
//! no_events_format = " $icon no events "
//! interval = 30
//! url = "https://caldav.example.com/calendar/"
//! calendars = ["user/calendar"]
//! events_within_hours = 48
//! warning_threshold = 600
//! browser_cmd = "firefox"
//! [block.auth]
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
//! interval = 30
//! url = "https://caldav.example.com/calendar/"
//! calendars = [ "Holidays" ]
//! events_within_hours = 48
//! warning_threshold = 600
//! browser_cmd = "firefox"
//! [block.auth]
//! type = "basic"
//! username = "your_username"
//! password = "your_password"
//! ```
//! Note: The `username` and `password` can also be provided by setting the environment variables `I3RS_CALENDAR_AUTH_USERNAME` and `I3RS_CALENDAR_PASSWORD` respectively.
//!
//! ## OAuth2 Authentication (Google Calendar)
//!
//! To access the CalDav API of Google, follow these steps to enable the API and obtain the `client_id`` and `client_secret`:
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
//! interval = 30
//! url = "https://apidata.googleusercontent.com/caldav/v2/"
//! calendars = ["primary"]
//! events_within_hours = 48
//! warning_threshold = 600
//! browser_cmd = "firefox"
//! [block.auth]
//! type = "oauth2"
//! client_id = "your_client_id"
//! client_secret = "your_client_secret"
//! auth_url = "https://accounts.google.com/o/oauth2/auth"
//! token_url = "https://oauth2.googleapis.com/token"
//! auth_token = "~/.config/i3status-rust/calendar.auth_token"
//! redirect_port = 8080
//! scopes = ["https://www.googleapis.com/auth/calendar", "https://www.googleapis.com/auth/calendar.events"]
//! ```
//! Note: The `client_id` and `client_secret` can also be provided by setting the environment variables `I3RS_CALENDAR_AUTH_USERNAME` and `I3RS_CALENDAR_PASSWORD` respectively.
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

use chrono::{Duration, Utc};
use oauth2::{AuthUrl, ClientId, ClientSecret, TokenUrl, Scope};
use url::Url;

use crate::{subprocess::spawn_process, util::has_command};

mod auth;
mod caldav;

use self::auth::{AuthorizeUrl, OAuth2Flow, TokenStore};
use self::caldav::Event;

use super::prelude::*;

use std::path::Path;

use caldav::CalDavClient;

#[derive(Deserialize, Debug)]
pub struct BasicAuthConfig {
    username: Option<String>,
    password: Option<String>,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct OAuth2Config {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    #[default("~/.config/i3status-rust/calendar.auth_token".into())]
    pub auth_token: ShellString,
    #[default(8080)]
    pub redirect_port: u16,
    pub scopes: Vec<Scope>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthConfig {
    #[default]
    Unauthenticated,
    Basic(BasicAuthConfig),
    OAuth2(OAuth2Config),
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub next_event_format: FormatConfig,
    pub ongoing_event_format: FormatConfig,
    pub no_events_format: FormatConfig,
    pub redirect_format: FormatConfig,
    #[default(60.into())]
    pub interval: Seconds,
    pub url: String,
    pub auth: AuthConfig,
    pub calendars: Vec<String>,
    #[default(48)]
    pub events_within_hours: u64,
    #[default(300.into())]
    pub warning_threshold: Seconds,
    #[default("xdg-open".into())]
    pub browser_cmd: ShellString,
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

    let mut client = caldav_client(config).await?;

    let mut timer = config.interval.timer();

    let mut actions = api.get_actions()?;

    loop {
        let mut widget = Widget::new().with_format(no_events_format.clone());
        widget.set_values(map! {
            "icon" => Value::icon("calendar"),
        });

        let mut next_event = None;
        let mut retries = 0;
        while retries <= 1 {
            let next_event_result = get_next_event(config, &mut client).await;
            match next_event_result {
                Ok(event) => {
                    next_event = event;
                    break;
                }
                Err(err) => {
                    if let CalendarError::AuthRequired = err {
                        let authorization =
                            client.authorize().await.error("Authorization failed")?;
                        match &authorization {
                            auth::Authorize::Completed => {
                                return Err(Error::new(
                                    "Authorization failed. Check your configurations",
                                ))
                            }
                            auth::Authorize::AskUser(AuthorizeUrl { url, .. }) => {
                                widget.set_format(redirect_format.clone());
                                api.set_widget(widget.clone())?;
                                open_browser(config, url).await?;
                                client
                                    .ask_user(authorization)
                                    .await
                                    .error("Ask user failed")?;
                            }
                        }
                    }
                }
            };
            retries += 1;
        }
        if let Some(event) = next_event.clone() {
            if let (Some(start_date), Some(end_date)) = (event.end_at, event.end_at) {
                let warn_datetime = Utc::now()
                    - Duration::try_seconds(config.warning_threshold.seconds().try_into().unwrap())
                        .unwrap();
                if warn_datetime < start_date && start_date < Utc::now() {
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
              _ = timer.tick() => break,
              _ = api.wait_for_update_request() => break,
              Some(action) = actions.recv() => match action.as_ref() {
                    "open_link" => {
                            if let Some(url) = next_event.clone().and_then(|e| e.url) {
                                if let Ok(url) = Url::parse(&url) {
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

async fn open_browser(config: &Config, url: &Url) -> Result<()> {
    let cmd = config.browser_cmd.expand()?;
    has_command(&cmd)
        .await
        .or_error(|| "Browser command not found")?;
    spawn_process(&cmd, &[url.as_ref()]).error("Open browser failed")
}

async fn caldav_client(config: &Config) -> Result<caldav::CalDavClient> {
    let auth = match &config.auth {
        AuthConfig::Unauthenticated => auth::Auth::Unauthenticated,
        AuthConfig::Basic(BasicAuthConfig { username, password }) => {
            let username = username
                .clone()
                .or_else(|| std::env::var("I3RS_CALENDAR_AUTH_USERNAME").ok())
                .error("Calendar username not found")?;
            let password = password
                .clone()
                .or_else(|| std::env::var("I3RS_CALENDAR_AUTH_PASSWORD").ok())
                .error("Calendar password not found")?;
            auth::Auth::basic(username, password)
        }
        AuthConfig::OAuth2(oauth2) => {
            let client_id = oauth2
                .client_id
                .clone()
                .or_else(|| std::env::var("I3RS_CALENDAR_AUTH_CLIENT_ID").ok())
                .error("Calendar oauth2 client_id not found")?;
            let client_secret = oauth2
                .client_secret
                .clone()
                .or_else(|| std::env::var("I3RS_CALENDAR_AUTH_CLIENT_SECRET").ok())
                .error("Calendar oauth2 client_secret not found")?;
            let auth_url =
                AuthUrl::new(oauth2.auth_url.clone()).error("Invalid authorization url")?;
            let token_url = TokenUrl::new(oauth2.token_url.clone()).error("Invalid token url")?;

            let flow = OAuth2Flow::new(
                ClientId::new(client_id),
                ClientSecret::new(client_secret),
                auth_url,
                token_url,
                oauth2.redirect_port,
            );
            let token_store = TokenStore::new(Path::new(&oauth2.auth_token.expand()?.to_string()));
            auth::Auth::oauth2(flow, token_store, oauth2.scopes.clone())
        }
    };
    Ok(CalDavClient::new(
        Url::parse(&config.url).error("Invalid CalDav server url")?,
        auth,
    ))
}

async fn get_next_event(
    config: &Config,
    client: &mut CalDavClient,
) -> Result<Option<caldav::Event>, CalendarError> {
    let calendars: Vec<_> = client
        .calendars()
        .await?
        .into_iter()
        .filter(|c| config.calendars.is_empty() || config.calendars.contains(&c.name))
        .collect();
    let mut events: Vec<Event> = vec![];
    for calendar in calendars {
        let calendar_events: Vec<_> = client
            .events(
                &calendar,
                Utc::now(),
                Utc::now()
                    + Duration::try_days(config.events_within_hours.try_into().unwrap()).unwrap(),
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
    Ok(events.first().cloned())
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
    #[error("Token not exchanged")]
    TokenNotExchanged,
    #[error("Request token error: {0}")]
    RequestToken(String),
}
