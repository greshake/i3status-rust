//! The current time.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | Format string. See [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options. | `" $icon %a %d/%m %R "`
//! `interval` | Update interval in seconds | `10`
//! `timezone` | A timezone specifier (e.g. "Europe/Lisbon") | Local timezone
//! `locale` | Locale to apply when formatting the time | System locale
//!
//! Placeholder   | Value                                       | Type   | Unit
//! --------------|---------------------------------------------|--------|-----
//! `icon`        | A static icon                               | Icon   | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "time"
//! interval = 60
//! locale = "fr_BE"
//! [block.format]
//! full = " $icon %d/%m %R "
//! short = " $icon %R "
//! ```
//!
//! # Icons Used
//! - `time`

use chrono::offset::{Local, Utc};
use chrono::Locale;
use chrono_tz::Tz;

use super::prelude::*;
use crate::formatting::config::DummyConfig;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    format: DummyConfig,
    #[default(1.into())]
    interval: Seconds,
    timezone: Option<Tz>,
    locale: Option<String>,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let mut widget = Widget::new();

    let format = config
        .format
        .full
        .as_deref()
        .unwrap_or(" $icon %a %d/%m %R ");
    let format_short = config.format.short.as_deref();

    let timezone = config.timezone;
    let locale = match config.locale.as_deref() {
        Some(locale) => Some(locale.try_into().ok().error("invalid locale")?),
        None => None,
    };

    let mut timer = config.interval.timer();

    loop {
        if timezone.is_none() {
            // Update timezone because `chrono` will not do that for us.
            // https://github.com/chronotope/chrono/issues/272
            unsafe { tzset() };
        }

        let full_time = get_time(format, timezone, locale);
        let short_time = format_short
            .map(|f| get_time(f, timezone, locale))
            .unwrap_or_else(|| "".into());

        widget.set_format(FormatConfig::default().with_defaults(&full_time, &short_time)?);
        widget.set_values(map!("icon" => Value::icon(api.get_icon("time")?)));

        api.set_widget(&widget).await?;

        tokio::select! {
            _ = timer.tick() => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

fn get_time(format: &str, timezone: Option<Tz>, locale: Option<Locale>) -> String {
    match locale {
        Some(locale) => match timezone {
            Some(tz) => Utc::now()
                .with_timezone(&tz)
                .format_localized(format, locale)
                .to_string(),
            None => Local::now().format_localized(format, locale).to_string(),
        },
        None => match timezone {
            Some(tz) => Utc::now().with_timezone(&tz).format(format).to_string(),
            None => Local::now().format(format).to_string(),
        },
    }
}

extern "C" {
    /// The tzset function initializes the tzname variable from the value of the TZ environment
    /// variable. It is not usually necessary for your program to call this function, because it is
    /// called automatically when you use the other time conversion functions that depend on the
    /// time zone.
    fn tzset();
}
