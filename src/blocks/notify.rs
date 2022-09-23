//! Display and toggle the state of notifications daemon
//!
//! Right now only `dunst` is supported.
//!
//! Left-clicking on this block will enable/disable notifications.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `driver` | Which notifications daemon is running. Available drivers are: `"dunst"` | `"dunst"`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon "`
//!
//! Placeholder | Value                                      | Type   | Unit
//! ------------|--------------------------------------------|--------|-----
//! `icon`      | A static icon                              | Icon   | -
//! `paused`    | Present only if notifications are disabled | Flag   | -
//!
//! # Examples
//!
//! How to use `paused` flag
//!
//! ```toml
//! [[block]]
//! block = "notify"
//! format = " $icon $paused{Off}|On "
//! ```
//!
//! # Icons Used
//! - `bell`
//! - `bell-slash`

use super::prelude::*;
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

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "lowercase")]
enum DriverType {
    #[default]
    Dunst,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = NotifyConfig::deserialize(config).config_error()?;
    let mut widget = api
        .new_widget()
        .with_format(config.format.with_default(" $icon ")?);

    let mut driver: Box<dyn Driver + Send + Sync> = match config.driver {
        DriverType::Dunst => Box::new(MakoDriver::new().await?),
    };

    loop {
        let is_paused = driver.is_paused().await?;
        let mut values = map!(
            "icon" => Value::icon(api.get_icon(if is_paused { ICON_OFF } else { ICON_ON })?)
        );

        if is_paused {
            values.insert("paused".into(), Value::flag());
        }

        widget.set_values(values);
        api.set_widget(&widget).await?;

        loop {
            select! {
                x = driver.wait_for_change() => {
                    x?;
                    break;
                }
                event = api.event() => match event {
                    UpdateRequest => break,
                    Click(click) => {
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

struct MakoDriver {
    proxy: DunstDbusProxy<'static>,
    changes: PropertyStream<'static, bool>,
}

impl MakoDriver {
    async fn new() -> Result<Self> {
        let dbus_conn = new_dbus_connection().await?;
        let proxy = DunstDbusProxy::new(&dbus_conn)
            .await
            .error("Failed to create DunstDbusProxy")?;
        Ok(Self {
            changes: proxy.receive_paused_changed().await,
            proxy,
        })
    }
}

#[async_trait]
impl Driver for MakoDriver {
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
