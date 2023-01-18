//! Display and toggle the state of notifications daemon
//!
//! Left-clicking on this block will enable/disable notifications.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `driver` | Which notifications daemon is running. Available drivers are: `"dunst"` and `"swaync"` | `"dunst"`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon "`
//!
//! Placeholder                               | Value                                      | Type   | Unit
//! ------------------------------------------|--------------------------------------------|--------|-----
//! `icon`                                    | Icon based on notification's state         | Icon   | -
//! `notification_count`[^dunst_version_note] | The number of notification (omitted if 0)  | Number | -
//! `paused`                                  | Present only if notifications are disabled | Flag   | -
//!
//! Action          | Default button
//! ----------------|---------------
//! `toggle_paused` | Left
//! `show`          | -
//!
//! # Examples
//!
//! How to use `paused` flag
//!
//! ```toml
//! [[block]]
//! block = "notify"
//! format = " $icon {$paused{Off}|On} "
//! ```
//! How to use `notification_count`
//!
//! ```toml
//! [[block]]
//! block = "notify"
//! format = " $icon {($notification_count.eng(1)) |}"
//! ```
//! How to remap actions
//!
//! ```toml
//! [[block]]
//! block = "notify"
//! driver = "swaync"
//! [[block.click]]
//! button = "left"
//! action = "show"
//! [[block.click]]
//! button = "right"
//! action = "toggle_paused"
//! ```
//!
//! # Icons Used
//! - `bell`
//! - `bell-slash`
//!
//! [^dunst_version_note]: when using `notification_count` with the `dunst` driver use dunst > 1.9.0

use super::prelude::*;
use tokio::try_join;
use zbus::dbus_proxy;
use zbus::PropertyStream;

const ICON_ON: &str = "bell";
const ICON_OFF: &str = "bell-slash";

#[derive(Deserialize, Debug, Default)]
#[serde(default)]
pub struct Config {
    driver: DriverType,
    format: FormatConfig,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "lowercase")]
enum DriverType {
    #[default]
    Dunst,
    SwayNC,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    api.set_default_actions(&[(MouseButton::Left, None, "toggle_paused")])
        .await?;

    let mut widget = Widget::new().with_format(config.format.with_default(" $icon ")?);

    let mut driver: Box<dyn Driver> = match config.driver {
        DriverType::Dunst => Box::new(DunstDriver::new().await?),
        DriverType::SwayNC => Box::new(SwayNCDriver::new().await?),
    };

    loop {
        let (is_paused, notification_count) =
            try_join!(driver.is_paused(), driver.notification_count())?;

        widget.set_values(map!(
            "icon" => Value::icon(api.get_icon(if is_paused { ICON_OFF } else { ICON_ON })?),
            [if notification_count != 0] "notification_count" => Value::number(notification_count),
            [if is_paused] "paused" => Value::flag(),
        ));
        widget.state = if notification_count == 0 {
            State::Idle
        } else {
            State::Info
        };
        api.set_widget(&widget).await?;

        select! {
            x = driver.wait_for_change() => x?,
            event = api.event() => match event {
                Action(a) if a == "toggle_paused" => {
                    driver.set_paused(!is_paused).await?;
                }
                Action(a) if a == "show" => {
                    driver.notification_show().await?;
                }
                _ => (),
            }
        }
    }
}

#[async_trait]
trait Driver {
    async fn is_paused(&self) -> Result<bool>;
    async fn set_paused(&self, paused: bool) -> Result<()>;
    async fn notification_show(&self) -> Result<()>;
    async fn notification_count(&self) -> Result<u32>;
    async fn wait_for_change(&mut self) -> Result<()>;
}

struct DunstDriver {
    proxy: DunstDbusProxy<'static>,
    paused_changes: PropertyStream<'static, bool>,
    displayed_length_changes: PropertyStream<'static, u32>,
    waiting_length_changes: PropertyStream<'static, u32>,
}

impl DunstDriver {
    async fn new() -> Result<Self> {
        let dbus_conn = new_dbus_connection().await?;
        let proxy = DunstDbusProxy::new(&dbus_conn)
            .await
            .error("Failed to create DunstDbusProxy")?;
        Ok(Self {
            paused_changes: proxy.receive_paused_changed().await,
            displayed_length_changes: proxy.receive_displayed_length_changed().await,
            waiting_length_changes: proxy.receive_waiting_length_changed().await,
            proxy,
        })
    }
}

#[async_trait]
impl Driver for DunstDriver {
    async fn is_paused(&self) -> Result<bool> {
        self.proxy.paused().await.error("Failed to get 'paused'")
    }

    async fn set_paused(&self, paused: bool) -> Result<()> {
        self.proxy
            .set_paused(paused)
            .await
            .error("Failed to set 'paused'")
    }

    async fn notification_show(&self) -> Result<()> {
        self.proxy
            .notification_show()
            .await
            .error("Could not call 'NotificationShow'")
    }

    async fn notification_count(&self) -> Result<u32> {
        let (displayed_length, waiting_length) =
            try_join!(self.proxy.displayed_length(), self.proxy.waiting_length())
                .error("Failed to get property")?;

        Ok(displayed_length + waiting_length)
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        select! {
            _ = self.paused_changes.next() => {}
            _ = self.displayed_length_changes.next() => {}
            _ = self.waiting_length_changes.next() => {}
        }
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
    fn notification_show(&self) -> zbus::Result<()>;
    #[dbus_proxy(property, name = "displayedLength")]
    fn displayed_length(&self) -> zbus::Result<u32>;
    #[dbus_proxy(property, name = "waitingLength")]
    fn waiting_length(&self) -> zbus::Result<u32>;
}
struct SwayNCDriver {
    proxy: SwayNCDbusProxy<'static>,
    changes: SubscribeStream<'static>,
}

impl SwayNCDriver {
    async fn new() -> Result<Self> {
        let dbus_conn = new_dbus_connection().await?;
        let proxy = SwayNCDbusProxy::new(&dbus_conn)
            .await
            .error("Failed to create SwayNCDbusProxy")?;
        Ok(Self {
            changes: proxy
                .receive_subscribe()
                .await
                .error("Failed to create SubscribeStream")?,
            proxy,
        })
    }
}

#[async_trait]
impl Driver for SwayNCDriver {
    async fn is_paused(&self) -> Result<bool> {
        self.proxy.get_dnd().await.error("Failed to call 'GetDnd'")
    }

    async fn set_paused(&self, paused: bool) -> Result<()> {
        self.proxy
            .set_dnd(paused)
            .await
            .error("Failed to call 'SetDnd'")
    }

    async fn notification_show(&self) -> Result<()> {
        self.proxy
            .toggle_visibility()
            .await
            .error("Failed to call 'ToggleVisibility'")
    }

    async fn notification_count(&self) -> Result<u32> {
        self.proxy
            .notification_count()
            .await
            .error("Failed to call 'NotificationCount'")
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        self.changes.next().await;
        Ok(())
    }
}

#[dbus_proxy(
    interface = "org.erikreider.swaync.cc",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/erikreider/swaync/cc"
)]
trait SwayNCDbus {
    fn get_dnd(&self) -> zbus::Result<bool>;
    fn set_dnd(&self, value: bool) -> zbus::Result<()>;
    fn toggle_visibility(&self) -> zbus::Result<()>;
    fn notification_count(&self) -> zbus::Result<u32>;
    #[dbus_proxy(signal)]
    fn subscribe(&self, value: bool) -> zbus::Result<()>;
}
