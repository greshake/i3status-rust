//! [KDEConnect](https://community.kde.org/KDEConnect) indicator
//!
//! Display info from the currently connected device in KDEConnect, updated asynchronously.
//!
//! Block colours are updated based on the battery level, unless all bat_* thresholds are set to 0,
//! in which case the block colours will depend on the notification count instead.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `device_id` | Device ID as per the output of `kdeconnect --list-devices`. | Chooses the first found device, if any.
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>" $icon $name{ $bat_icon $bat_charge&vert;}{ $notif_icon&vert;} "</code>
//! `bat_info` | Min battery level below which state is set to info. | `60`
//! `bat_good` | Min battery level below which state is set to good. | `60`
//! `bat_warning` | Min battery level below which state is set to warning. | `30`
//! `bat_critical` | Min battery level below which state is set to critical. | `15`
//! `hide_disconnected` | Whether to hide this block when disconnected | `true`
//!
//! Placeholder        | Value                                                                    | Type   | Unit
//! -------------------|--------------------------------------------------------------------------|--------|-----
//! `icon`             | Icon based on connection's status                                        | Icon   | -
//! `bat_icon`         | Battery level indicator (only when connected and if supported)           | Icon   | -
//! `bat_charge`       | Battery charge level (only when connected and if supported)              | Number | %
//! `network_icon`     | Cell Network indicator (only when connected and if supported)            | Icon   | -
//! `network_type`     | Cell Network type (only when connected and if supported)                 | Text   | -
//! `network_strength` | Cell Network level (only when connected and if supported)                | Number | %
//! `notif_icon`       | Only when connected and there are notifications                          | Icon   | -
//! `notif_count`      | Number of notifications on your phone (only when connected and non-zero) | Number | -
//! `name`             | Name of your device as reported by KDEConnect (if available)             | Text   | -
//! `connected`        | Present if your device is connected                                      | Flag   | -
//!
//! # Example
//!
//! Do not show the name, do not set the "good" state.
//!
//! ```toml
//! [[block]]
//! block = "kdeconnect"
//! format = " $icon {$bat_icon $bat_charge |}{$notif_icon |}{$network_icon$network_strength $network_type |}"
//! bat_good = 101
//! ```
//!
//! # Icons Used
//! - `bat` (as a progression)
//! - `bat_charging` (as a progression)
//! - `net_cellular` (as a progression)
//! - `notification`
//! - `phone`
//! - `phone_disconnected`

use futures::TryFutureExt;
use tokio::sync::mpsc;
use zbus::dbus_proxy;

use super::prelude::*;

mod battery;
mod connectivity_report;
use battery::BatteryDbusProxy;
use connectivity_report::ConnectivityDbusProxy;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub device_id: Option<String>,
    pub format: FormatConfig,
    #[default(60)]
    pub bat_good: u8,
    #[default(60)]
    pub bat_info: u8,
    #[default(30)]
    pub bat_warning: u8,
    #[default(15)]
    pub bat_critical: u8,
    #[default(true)]
    pub hide_disconnected: bool,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let format = config
        .format
        .with_default(" $icon $name {$bat_icon $bat_charge |}{$notif_icon |}")?;

    let battery_state = (
        config.bat_good,
        config.bat_info,
        config.bat_warning,
        config.bat_critical,
    ) != (0, 0, 0, 0);

    let dbus_conn = new_dbus_connection().await?;
    let id = match config.device_id {
        Some(id) => id,
        None => api.recoverable(|| any_device_id(&dbus_conn)).await?,
    };

    let (tx, mut rx) = mpsc::channel(8);
    let device = Device::new(&dbus_conn, tx, &id).await?;

    loop {
        let connected = device.connected().await;

        if connected || !config.hide_disconnected {
            let mut widget = Widget::new().with_format(format.clone());

            let mut values = map!();

            if let Some(name) = device.name().await {
                values.insert("name".into(), Value::text(name));
            }

            if connected {
                values.insert("icon".into(), Value::icon(api.get_icon("phone")?));
                values.insert("connected".into(), Value::flag());
                let (
                    (level, charging),
                    (cellular_network_type, cellular_network_strength),
                    notif_count,
                ) = tokio::join!(device.battery(), device.network(), device.notifications());

                if let Ok(level) = level {
                    values.insert("bat_charge".into(), Value::percents(level));
                    values.insert(
                        "bat_icon".into(),
                        Value::icon(api.get_icon_in_progression(
                            if charging { "bat_charging" } else { "bat" },
                            level as f64 / 100.0,
                        )?),
                    );
                    if battery_state {
                        widget.state = if charging {
                            State::Good
                        } else if level <= config.bat_critical {
                            State::Critical
                        } else if level <= config.bat_info {
                            State::Info
                        } else if level > config.bat_good {
                            State::Good
                        } else {
                            State::Idle
                        };
                    }
                }

                if let Ok(cellular_network_type) = cellular_network_type {
                    // network strength is 0..=4 from docs of
                    // kdeconnect/plugins/connectivity-report, and I
                    // got -1 for disabled SIM (undocumented)
                    let cell_network_percent = (cellular_network_strength.clamp(0, 4) * 25) as f64;
                    values.insert(
                        "network_icon".into(),
                        Value::icon(api.get_icon_in_progression(
                            "net_cellular",
                            (cellular_network_strength + 1).clamp(0, 5) as f64 / 5.0,
                        )?),
                    );
                    values.insert(
                        "network_strength".into(),
                        Value::percents(cell_network_percent),
                    );

                    if cellular_network_strength <= 0 {
                        widget.state = State::Critical;
                        values.insert("network_type".into(), Value::text("×".into()));
                    } else {
                        values.insert("network_type".into(), Value::text(cellular_network_type));
                    }
                }

                if notif_count > 0 {
                    values.insert("notif_count".into(), Value::number(notif_count));
                    values.insert(
                        "notif_icon".into(),
                        Value::icon(api.get_icon("notification")?),
                    );
                }
                if !battery_state {
                    widget.state = if notif_count == 0 {
                        State::Idle
                    } else {
                        State::Info
                    };
                }
            } else {
                values.insert(
                    "icon".into(),
                    Value::icon(api.get_icon("phone_disconnected")?),
                );
            }

            widget.set_values(values);
            api.set_widget(widget).await?;
        } else {
            api.hide().await?;
        }

        loop {
            select! {
                _ = rx.recv() => break,
                _ = api.event() => (),
            }
        }
    }
}

struct Device {
    device_proxy: DeviceDbusProxy<'static>,
    battery_proxy: BatteryDbusProxy<'static>,
    notifications_proxy: NotificationsDbusProxy<'static>,
    connectivity_proxy: ConnectivityDbusProxy<'static>,
}

impl Device {
    async fn new(conn: &zbus::Connection, tx: mpsc::Sender<()>, id: &str) -> Result<Self> {
        let device_path = format!("/modules/kdeconnect/devices/{id}");
        let battery_path = format!("{device_path}/battery");
        let notifications_path = format!("{device_path}/notifications");
        let connectivity_path = format!("{device_path}/connectivity_report");

        let device_proxy = DeviceDbusProxy::builder(conn)
            .cache_properties(zbus::CacheProperties::No)
            .path(device_path)
            .error("Failed to set device path")?
            .build()
            .await
            .error("Failed to create DeviceDbusProxy")?;
        let battery_proxy = BatteryDbusProxy::builder(conn)
            .cache_properties(zbus::CacheProperties::No)
            .path(battery_path)
            .error("Failed to set battery path")?
            .build()
            .await
            .error("Failed to create BatteryDbusProxy")?;
        let notifications_proxy = NotificationsDbusProxy::builder(conn)
            .cache_properties(zbus::CacheProperties::No)
            .path(notifications_path)
            .error("Failed to set battery path")?
            .build()
            .await
            .error("Failed to create BatteryDbusProxy")?;
        let connectivity_proxy = ConnectivityDbusProxy::builder(conn)
            .cache_properties(zbus::CacheProperties::No)
            .path(connectivity_path)
            .error("Failed to set connectivity path")?
            .build()
            .await
            .error("Failed to create ConnectivityDbusProxy")?;

        let mut s1 = device_proxy
            .receive_all_signals()
            .await
            .error("Failed to receive signals")?;
        let mut s2 = battery_proxy
            .receive_refreshed()
            .await
            .error("Failed to receive signals")?;
        let mut s3 = notifications_proxy
            .receive_all_signals()
            .await
            .error("Failed to receive signals")?;
        let mut s4 = connectivity_proxy
            .receive_refreshed()
            .await
            .error("Failed to receive signals")?;

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = s1.next() => tx.send(()).await.unwrap(),
                    _ = s2.next() => tx.send(()).await.unwrap(),
                    _ = s3.next() => tx.send(()).await.unwrap(),
                    _ = s4.next() => tx.send(()).await.unwrap(),
                }
            }
        });

        Ok(Self {
            device_proxy,
            battery_proxy,
            notifications_proxy,
            connectivity_proxy,
        })
    }

    async fn connected(&self) -> bool {
        self.device_proxy.is_reachable().await.unwrap_or(false)
    }

    async fn name(&self) -> Option<String> {
        self.device_proxy.name().await.ok()
    }

    async fn battery(&self) -> (Result<u8>, bool) {
        tokio::join!(
            self.battery_proxy
                .charge()
                .map_ok(|p| p.clamp(0, 100) as u8)
                .map_err(|_| Error::new("Could not get charge")),
            self.battery_proxy
                .is_charging()
                .map_ok_or_else(|_| false, |x| x),
        )
    }

    async fn notifications(&self) -> usize {
        self.notifications_proxy
            .active_notifications()
            .await
            .map(|n| n.len())
            .unwrap_or(0)
    }

    async fn network(&self) -> (Result<String>, i32) {
        tokio::join!(
            self.connectivity_proxy
                .cellular_network_type()
                .map_err(|_| Error::new("Could not get cellular_network_type")),
            self.connectivity_proxy
                .cellular_network_strength()
                .map_ok_or_else(|_| -1, |x| x),
        )
    }
}

async fn any_device_id(conn: &zbus::Connection) -> Result<String> {
    DaemonDbusProxy::new(conn)
        .await
        .error("Failed to create DaemonDbusProxy")?
        .devices()
        .await
        .error("Failed to get devices")?
        .into_iter()
        .next()
        .error("No devices found")
}

#[dbus_proxy(
    interface = "org.kde.kdeconnect.daemon",
    default_service = "org.kde.kdeconnect",
    default_path = "/modules/kdeconnect"
)]
trait DaemonDbus {
    #[dbus_proxy(name = "devices")]
    fn devices(&self) -> zbus::Result<Vec<String>>;
}

#[dbus_proxy(
    interface = "org.kde.kdeconnect.device",
    default_service = "org.kde.kdeconnect"
)]
trait DeviceDbus {
    #[dbus_proxy(property, name = "isReachable")]
    fn is_reachable(&self) -> zbus::Result<bool>;

    #[dbus_proxy(signal, name = "reachableChanged")]
    fn reachable_changed(&self, reachable: bool) -> zbus::Result<()>;

    #[dbus_proxy(property, name = "name")]
    fn name(&self) -> zbus::Result<String>;

    #[dbus_proxy(signal, name = "nameChanged")]
    fn name_changed_(&self, name: &str) -> zbus::Result<()>;
}

#[dbus_proxy(
    interface = "org.kde.kdeconnect.device.notifications",
    default_service = "org.kde.kdeconnect"
)]
trait NotificationsDbus {
    #[dbus_proxy(name = "activeNotifications")]
    fn active_notifications(&self) -> zbus::Result<Vec<String>>;

    #[dbus_proxy(signal, name = "allNotificationsRemoved")]
    fn all_notifications_removed(&self) -> zbus::Result<()>;

    #[dbus_proxy(signal, name = "notificationPosted")]
    fn notification_posted(&self, id: &str) -> zbus::Result<()>;

    #[dbus_proxy(signal, name = "notificationRemoved")]
    fn notification_removed(&self, id: &str) -> zbus::Result<()>;
}
