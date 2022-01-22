//! Display and toggle the state of notifications daemon
//!
//! Right now only `dunst` is supported.
//!
//! Left-clicking on this block will enable/disable notifications.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `driver` | Which notifications daemon is running. Available drivers are: `"dunst"` | No | `"dunst"`
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `""`
//!
//! Placeholder | Value                                      | Type   | Unit
//! ------------|--------------------------------------------|--------|-----
//! `paused`    | Present only if notifications are disabled | Flag   | -
//!
//! # Examples
//!
//! How to use `paused` flag
//!
//! ```toml
//! [block]
//! block = "notify"
//! format = "$paused{Off}|On"
//! ```
//!
//! # Icons Used
//! - `bell`
//! - `bell-slash`

use super::prelude::*;
use async_trait::async_trait;
use std::collections::HashMap;
use zbus::dbus_proxy;
use zbus::PropertyStream;

const ICON_ON: &str = "bell";
const ICON_OFF: &str = "bell-slash";

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields, default)]
struct NotifyConfig {
    driver: DriverType,
    format: FormatConfig,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum DriverType {
    Dunst,
}

impl Default for DriverType {
    fn default() -> Self {
        Self::Dunst
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let mut events = api.get_events().await?;
    let config = NotifyConfig::deserialize(config).config_error()?;
    api.set_format(config.format.with_default("")?);

    let dbus_conn = api.get_dbus_connection().await?;
    let mut driver: Box<dyn Driver + Send + Sync> = match config.driver {
        DriverType::Dunst => Box::new(MakoDriver::new(&dbus_conn).await?),
    };

    loop {
        let is_paused = driver.is_paused().await?;

        api.set_icon(if is_paused { ICON_OFF } else { ICON_ON })?;

        let mut values = HashMap::new();
        if is_paused {
            values.insert("paused".into(), Value::Flag);
        }
        api.set_values(values);
        api.flush().await?;

        loop {
            tokio::select! {
                x = driver.wait_for_change() => {
                    x?;
                    break;
                }
                event = events.recv() => {
                    if let BlockEvent::Click(click) = event.unwrap() {
                        if click.button == MouseButton::Left {
                            driver.set_paused(!is_paused).await?;
                        }
                    }
                }
            }
        }
    }
}

#[async_trait]
trait Driver {
    async fn is_paused(&self) -> Result<bool>;
    async fn set_paused(&self, paused: bool) -> Result<()>;
    async fn wait_for_change(&mut self) -> Result<()>;
}

struct MakoDriver<'a> {
    proxy: DunstDbusProxy<'a>,
    changes: PropertyStream<'static, bool>,
}

impl<'a> MakoDriver<'a> {
    async fn new(dbus_conn: &zbus::Connection) -> Result<MakoDriver<'a>> {
        let proxy = DunstDbusProxy::new(dbus_conn)
            .await
            .error("Failed to create DunstDbusProxy")?;
        Ok(Self {
            changes: proxy.receive_paused_changed().await,
            proxy,
        })
    }
}

#[async_trait]
impl<'a> Driver for MakoDriver<'a> {
    async fn is_paused(&self) -> Result<bool> {
        self.proxy.paused().await.error("Failed to get 'paused'")
    }

    async fn set_paused(&self, paused: bool) -> Result<()> {
        self.proxy
            .set_paused(paused)
            .await
            .error("Failed to set 'paused'")
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        self.changes.next().await;
        Ok(())
    }
}

#[dbus_proxy(
    interface = "org.dunstproject.cmd0",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait DunstDbus {
    #[dbus_proxy(property, name = "paused")]
    fn paused(&self) -> zbus::Result<bool>;
    #[dbus_proxy(property, name = "paused")]
    fn set_paused(&self, value: bool) -> zbus::Result<()>;
}
