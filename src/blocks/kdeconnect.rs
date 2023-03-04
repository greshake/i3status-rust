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
//! Placeholder   | Value                                                                    | Type   | Unit
//! --------------|--------------------------------------------------------------------------|--------|-----
//! `icon`        | Icon based on connection's status                                        | Icon   | -
//! `bat_icon`    | Battery level indicator (only when connected and if supported)           | Icon   | -
//! `bat_charge`  | Battery charge level (only when connected and if supported)              | Number | %
//! `notif_icon`  | Only when connected and there are notifications                          | Icon   | -
//! `notif_count` | Number of notifications on your phone (only when connected and non-zero) | Number | -
//! `name`        | Name of your device as reported by KDEConnect (if available)             | Text   | -
//! `connected`   | Present if your device is connected                                      | Flag   | -
//!
//! # Example
//!
//! Do not show the name, do not set the "good" state.
//!
//! ```toml
//! [[block]]
//! block = "kdeconnect"
//! format = " $icon {$bat_icon $bat_charge|}{ $notif_icon|} "
//! bat_good = 101
//! ```
//!
//! # Icons Used
//! - `bat` (as a progression)
//! - `bat_charging` (as a progression)
//! - `notification`
//! - `phone`
//! - `phone_disconnected`

use zbus::{dbus_proxy, SignalStream};

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    device_id: Option<String>,
    format: FormatConfig,
    #[default(60)]
    bat_good: u8,
    #[default(60)]
    bat_info: u8,
    #[default(30)]
    bat_warning: u8,
    #[default(15)]
    bat_critical: u8,
    #[default(true)]
    hide_disconnected: bool,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let mut widget = Widget::new().with_format(
        config
            .format
            .with_default(" $icon $name{ $bat_icon $bat_charge|}{ $notif_icon|} ")?,
    );

    let battery_state = (
        config.bat_good,
        config.bat_info,
        config.bat_warning,
        config.bat_critical,
    ) != (0, 0, 0, 0);

    let mut monitor = match config.device_id {
        Some(id) => DeviceMonitor::from_id(&id).await?,
        None => DeviceMonitor::new(),
    };

    loop {
        match monitor.get_device_info().await {
            Some(device) => {
                if !config.hide_disconnected {
                    widget.state = State::Idle;
                }

                let mut values = map!();

                if let Some(name) = device.name().await {
                    values.insert("name".into(), Value::text(name));
                }

                if device.connected().await {
                    values.insert("icon".into(), Value::icon(api.get_icon("phone")?));
                    values.insert("connected".into(), Value::flag());

                    let (level, charging) = device.battery().await;
                    if let Some(level) = level {
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

                    let notif_count = device.notifications().await?;
                    if notif_count > 0 {
                        values.insert("notif_count".into(), Value::number(notif_count));
                        values.insert(
                            "notif_icon".into(),
                            Value::icon(api.get_icon("notification")?.trim().into()),
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
                api.set_widget(&widget).await?;
            }
            None => {
                api.hide().await?;
                select! {
                    _ = monitor.wait_for_change() => (),
                    _ = api.event() => (),
                }
            }
        }
    }
}

struct DeviceMonitor {
    device: Option<Device>,
}

impl DeviceMonitor {
    pub fn new() -> Self {
        Self { device: None }
    }

    pub async fn from_id(id: &str) -> Result<DeviceMonitor> {
        let device = device_from_id(id).await?;
        let monitor = DeviceMonitor {
            device: Some(device),
        };
        Ok(monitor)
    }

    pub async fn get_device_info(&mut self) -> Option<&Device> {
        match &self.device {
            Some(_) => self.device.as_ref(),
            None => None,
        }
    }

    pub async fn wait_for_change(&mut self) -> Option<()> {
        match &self.device {
            Some(_) => {
                let device = &mut self.device.as_mut()?;

                let notification_event = device.notification_stream.next();

                let battery_event = device.battery_stream.next();

                let device_event = device.device_stream.next();

                tokio::select! {
                    _ = notification_event => {
                        Some(())
                    }
                    _ = battery_event => {
                        Some(())
                    }
                    _ = device_event => {
                        Some(())
                    }
                }
            }
            None => match get_new_device().await {
                Ok(device) => {
                    self.device = Some(device);
                    None
                }
                Err(_) => None,
            },
        }
    }
}

async fn get_new_device() -> Result<Device> {
    let conn = new_dbus_connection().await?;

    let ids = any_device_id(&conn).await?;

    for id in ids {
        let device_path = format!("/modules/kdeconnect/devices/{id}");
        let device_proxy = DeviceDbusProxy::builder(&conn)
            .cache_properties(zbus::CacheProperties::No)
            .path(device_path)
            .error("Failed to set device path")?
            .build()
            .await
            .error("Failed to create DeviceDbusProxy")?;

        match device_proxy.is_reachable().await {
            Ok(res) => {
                if res {
                    let device = Device::new(&conn, &id).await?;
                    return Ok(device);
                }
            }
            Err(_) => continue,
        }
    }

    Err(Error::new("Could not connect to any device"))
}

async fn device_from_id(id: &str) -> Result<Device> {
    let conn = new_dbus_connection().await?;

    let device = Device::new(&conn, id).await?;

    Ok(device)
}

struct Device {
    pub device_stream: SignalStream<'static>,
    pub battery_stream: refreshedStream<'static>,
    pub notification_stream: SignalStream<'static>,
    device_proxy: DeviceDbusProxy<'static>,
    battery_proxy: BatteryDbusProxy<'static>,
    notifications_proxy: NotificationsDbusProxy<'static>,
}

impl Device {
    async fn new(conn: &zbus::Connection, id: &str) -> Result<Device> {
        let device_path = format!("/modules/kdeconnect/devices/{id}");
        let battery_path = format!("{device_path}/battery");
        let notifications_path = format!("{device_path}/notifications");

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

        let device_stream = device_proxy
            .receive_all_signals()
            .await
            .error("Failed to receive signals")?;
        let battery_stream = battery_proxy
            .receive_refreshed()
            .await
            .error("Failed to receive signals")?;
        let notification_stream = notifications_proxy
            .receive_all_signals()
            .await
            .error("Failed to receive signals")?;

        Ok(Self {
            device_proxy,
            battery_proxy,
            notifications_proxy,
            device_stream,
            battery_stream,
            notification_stream,
        })
    }

    async fn connected(&self) -> bool {
        self.device_proxy.is_reachable().await.unwrap_or(false)
    }

    async fn name(&self) -> Option<String> {
        self.device_proxy.name().await.ok()
    }

    async fn battery(&self) -> (Option<u8>, bool) {
        (
            self.battery_proxy
                .charge()
                .await
                .ok()
                .map(|p| p.clamp(0, 100) as u8),
            self.battery_proxy.is_charging().await.unwrap_or(false),
        )
    }

    async fn notifications(&self) -> Result<usize> {
        self.notifications_proxy
            .active_notifications()
            .await
            .error("Failed to read notifications")
            .map(|n| n.len())
    }
}

async fn any_device_id(conn: &zbus::Connection) -> Result<std::vec::IntoIter<String>> {
    Ok(DaemonDbusProxy::new(conn)
        .await
        .error("Failed to create DaemonDbusProxy")?
        .devices()
        .await
        .error("Failed to get devices")?
        .into_iter())
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
    interface = "org.kde.kdeconnect.device.battery",
    default_service = "org.kde.kdeconnect"
)]
trait BatteryDbus {
    #[dbus_proxy(signal, name = "refreshed")]
    fn refreshed(&self, is_charging: bool, charge: i32) -> zbus::Result<()>;

    #[dbus_proxy(property, name = "charge")]
    fn charge(&self) -> zbus::Result<i32>;

    #[dbus_proxy(property, name = "isCharging")]
    fn is_charging(&self) -> zbus::Result<bool>;
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
