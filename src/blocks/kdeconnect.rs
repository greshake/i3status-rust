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
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>\" $icon $name{ $bat_icon $bat_charge\|}{ $notif_icon\|} \"</code>
//! `format_disconnected` | Same as `format` but when device is disconnected | `" $icon "`
//! `format_missing` | Same as `format` but when device does not exist | `" $icon x "`
//! `bat_info` | Min battery level below which state is set to info. | `60`
//! `bat_good` | Min battery level below which state is set to good. | `60`
//! `bat_warning` | Min battery level below which state is set to warning. | `30`
//! `bat_critical` | Min battery level below which state is set to critical. | `15`
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
//! - `bat` (`$bat_icon`, as a progression)
//! - `bat_charging` (`$bat_icon`, as a progression)
//! - `net_cellular` (`$network_icon`, as a progression)
//! - `notification`
//! - `phone` (`$icon`, in `format`)
//! - `phone_disconnected` (`$icon`, in `disconnected_format` `missing_format`)

use super::prelude::*;

mod battery;
mod connectivity_report;
use battery::BatteryDbusProxy;
use connectivity_report::ConnectivityDbusProxy;

make_log_macro!(debug, "kdeconnect");

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub device_id: Option<String>,
    pub format: FormatConfig,
    pub disconnected_format: FormatConfig,
    pub missing_format: FormatConfig,
    #[default(60)]
    pub bat_good: u8,
    #[default(60)]
    pub bat_info: u8,
    #[default(30)]
    pub bat_warning: u8,
    #[default(15)]
    pub bat_critical: u8,
}

pub(crate) fn prepare(config: &Config) -> Result<Arc<BlockPlan>> {
    Ok(BlockPlan::new(vec![
        OutputPlan::new(
            "connected",
            config
                .format
                .with_default(" $icon $name {$bat_icon $bat_charge |}{$notif_icon |}")?,
        )
        .icon("icon", IconChoices::one("phone"))
        .always_provides("icon", ValueKind::Icon)
        .icon("bat_icon", IconChoices::fixed(["bat", "bat_charging"]))
        .icon("network_icon", IconChoices::one("net_cellular"))
        .icon("notif_icon", IconChoices::one("notification")),
        OutputPlan::new(
            "disconnected",
            config.disconnected_format.with_default(" $icon ")?,
        )
        .icon("icon", IconChoices::one("phone_disconnected"))
        .always_provides("icon", ValueKind::Icon)
        .icon("bat_icon", IconChoices::fixed(["bat", "bat_charging"]))
        .icon("network_icon", IconChoices::one("net_cellular"))
        .icon("notif_icon", IconChoices::one("notification")),
        OutputPlan::new("missing", config.missing_format.with_default(" $icon x ")?)
            .icon("icon", IconChoices::one("phone_disconnected"))
        .always_provides("icon", ValueKind::Icon),
    ]))
}

pub async fn run(config: &Config, api: &CommonApi, plan: &Arc<BlockPlan>) -> Result<()> {
    let output_connected = plan.output("connected")?;
    let output_disconnected = plan.output("disconnected")?;
    let output_missing = plan.output("missing")?;

    let battery_state = (
        config.bat_good,
        config.bat_info,
        config.bat_warning,
        config.bat_critical,
    ) != (0, 0, 0, 0);

    let mut monitor = DeviceMonitor::new(config.device_id.clone()).await?;

    loop {
        match monitor.get_device_info().await {
            Some(info) => {
                let output = if info.connected {
                    &output_connected
                } else {
                    &output_disconnected
                };
                let mut widget = output.new_widget();

                let mut values = map! {
                    "icon" => output.icon_value("icon")?,
                    [if let Some(name) = info.name] "name" => Value::text(name),
                    [if info.notifications > 0] "notif_count" => Value::number(info.notifications),
                    [if info.notifications > 0] "notif_icon" => output.named_icon_value("notif_icon", "notification")?,
                    [if let Some(bat) = info.bat_level] "bat_charge" => Value::percents(bat),
                };

                if let Some(bat_level) = info.bat_level {
                    values.insert(
                        "bat_icon".into(),
                        output.icon_progression(
                            "bat_icon",
                            if info.charging { "bat_charging" } else { "bat" },
                            bat_level as f64 / 100.0,
                        )?,
                    );
                    if battery_state {
                        widget.state = if info.charging {
                            State::Good
                        } else if bat_level <= config.bat_critical {
                            State::Critical
                        } else if bat_level <= config.bat_info {
                            State::Info
                        } else if bat_level > config.bat_good {
                            State::Good
                        } else {
                            State::Idle
                        };
                    }
                }

                if !battery_state {
                    widget.state = if info.notifications == 0 {
                        State::Idle
                    } else {
                        State::Info
                    };
                }

                if let Some(cellular_network_type) = info.cellular_network_type {
                    // network strength is 0..=4 from docs of
                    // kdeconnect/plugins/connectivity-report, and I
                    // got -1 for disabled SIM (undocumented)
                    let cell_network_percent =
                        (info.cellular_network_strength.clamp(0, 4) * 25) as f64;
                    values.insert(
                        "network_icon".into(),
                        output.icon_progression(
                            "network_icon",
                            "net_cellular",
                            (info.cellular_network_strength + 1).clamp(0, 5) as f64 / 5.0,
                        )?,
                    );
                    values.insert(
                        "network_strength".into(),
                        Value::percents(cell_network_percent),
                    );

                    if info.cellular_network_strength <= 0 {
                        widget.state = State::Critical;
                        values.insert("network_type".into(), Value::text("×".into()));
                    } else {
                        values.insert("network_type".into(), Value::text(cellular_network_type));
                    }
                }

                widget.set_values(values);
                api.set_widget(widget)?;
            }
            None => {
                let mut widget = output_missing.new_widget();
                widget.set_values(map! { "icon" => output_missing.icon_value("icon")? });
                api.set_widget(widget)?;
            }
        }

        monitor.wait_for_change().await?;
    }
}

struct DeviceMonitor {
    device_id: Option<String>,
    daemon_proxy: DaemonDbusProxy<'static>,
    device: Option<Device>,
}

struct Device {
    id: String,
    device_proxy: DeviceDbusProxy<'static>,
    battery_proxy: BatteryDbusProxy<'static>,
    notifications_proxy: NotificationsDbusProxy<'static>,
    connectivity_proxy: ConnectivityDbusProxy<'static>,
    device_signals: zbus::proxy::SignalStream<'static>,
    notifications_signals: zbus::proxy::SignalStream<'static>,
    battery_refreshed: battery::refreshedStream,
    connectivity_refreshed: connectivity_report::refreshedStream,
}

struct DeviceInfo {
    connected: bool,
    name: Option<String>,
    notifications: usize,
    charging: bool,
    bat_level: Option<u8>,
    cellular_network_type: Option<String>,
    cellular_network_strength: i32,
}

impl DeviceMonitor {
    async fn new(device_id: Option<String>) -> Result<Self> {
        let dbus_conn = new_dbus_connection().await?;
        let daemon_proxy = DaemonDbusProxy::new(&dbus_conn)
            .await
            .error("Failed to create DaemonDbusProxy")?;
        let device = Device::try_find(&daemon_proxy, device_id.as_deref()).await?;
        Ok(Self {
            device_id,
            daemon_proxy,
            device,
        })
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        match &mut self.device {
            None => {
                let mut device_added = self
                    .daemon_proxy
                    .receive_device_added()
                    .await
                    .error("Couldn't create stream")?;
                loop {
                    device_added
                        .next()
                        .await
                        .error("Stream ended unexpectedly")?;
                    if let Some(device) =
                        Device::try_find(&self.daemon_proxy, self.device_id.as_deref()).await?
                    {
                        self.device = Some(device);
                        return Ok(());
                    }
                }
            }
            Some(dev) => {
                let mut device_removed = self
                    .daemon_proxy
                    .receive_device_removed()
                    .await
                    .error("Couldn't create stream")?;
                loop {
                    select! {
                        rem = device_removed.next() => {
                            let rem = rem.error("stream ended unexpectedly")?;
                            let args = rem.args().error("dbus error")?;
                            if args.id() == &dev.id {
                                self.device = Device::try_find(&self.daemon_proxy, self.device_id.as_deref()).await?;
                                return Ok(());
                            }
                        }
                        _ = dev.wait_for_change() => {
                            if !dev.connected().await {
                                debug!("device became unreachable, re-searching");
                                if let Some(dev) = Device::try_find(&self.daemon_proxy, self.device_id.as_deref()).await?
                                    && dev.connected().await {
                                        debug!("selected {:?}", dev.id);
                                        self.device = Some(dev);
                                    }
                            }
                            return Ok(())
                        }
                    }
                }
            }
        }
    }

    async fn get_device_info(&mut self) -> Option<DeviceInfo> {
        let device = self.device.as_ref()?;
        let (bat_level, charging) = device.battery().await;
        let (cellular_network_type, cellular_network_strength) = device.network().await;
        Some(DeviceInfo {
            connected: device.connected().await,
            name: device.name().await,
            notifications: device.notifications().await,
            charging,
            bat_level,
            cellular_network_type,
            cellular_network_strength,
        })
    }
}

impl Device {
    /// Find a device which `device_id`. Reachable devices have precedence.
    async fn try_find(
        daemon_proxy: &DaemonDbusProxy<'_>,
        device_id: Option<&str>,
    ) -> Result<Option<Self>> {
        let Ok(mut devices) = daemon_proxy.devices().await else {
            debug!("could not get the list of managed objects");
            return Ok(None);
        };

        debug!("all devices: {:?}", devices);

        if let Some(device_id) = device_id {
            devices.retain(|id| id == device_id);
        }

        let mut selected_device = None;

        for id in devices {
            let device_proxy = DeviceDbusProxy::builder(daemon_proxy.inner().connection())
                .cache_properties(zbus::proxy::CacheProperties::No)
                .path(format!("/modules/kdeconnect/devices/{id}"))
                .unwrap()
                .build()
                .await
                .error("Failed to create DeviceDbusProxy")?;
            let reachable = device_proxy.is_reachable().await.unwrap_or(false);
            selected_device = Some((id, device_proxy));
            if reachable {
                break;
            }
        }

        let Some((device_id, device_proxy)) = selected_device else {
            debug!("No device found");
            return Ok(None);
        };

        let device_path = format!("/modules/kdeconnect/devices/{device_id}");
        let battery_path = format!("{device_path}/battery");
        let notifications_path = format!("{device_path}/notifications");
        let connectivity_path = format!("{device_path}/connectivity_report");

        let battery_proxy = BatteryDbusProxy::builder(daemon_proxy.inner().connection())
            .cache_properties(zbus::proxy::CacheProperties::No)
            .path(battery_path)
            .error("Failed to set battery path")?
            .build()
            .await
            .error("Failed to create BatteryDbusProxy")?;
        let notifications_proxy =
            NotificationsDbusProxy::builder(daemon_proxy.inner().connection())
                .cache_properties(zbus::proxy::CacheProperties::No)
                .path(notifications_path)
                .error("Failed to set notifications path")?
                .build()
                .await
                .error("Failed to create BatteryDbusProxy")?;
        let connectivity_proxy = ConnectivityDbusProxy::builder(daemon_proxy.inner().connection())
            .cache_properties(zbus::proxy::CacheProperties::No)
            .path(connectivity_path)
            .error("Failed to set connectivity path")?
            .build()
            .await
            .error("Failed to create ConnectivityDbusProxy")?;

        let device_signals = device_proxy
            .inner()
            .receive_all_signals()
            .await
            .error("Failed to receive signals")?;
        let notifications_signals = notifications_proxy
            .inner()
            .receive_all_signals()
            .await
            .error("Failed to receive signals")?;
        let battery_refreshed = battery_proxy
            .receive_refreshed()
            .await
            .error("Failed to receive signals")?;
        let connectivity_refreshed = connectivity_proxy
            .receive_refreshed()
            .await
            .error("Failed to receive signals")?;

        Ok(Some(Self {
            id: device_id,
            device_proxy,
            battery_proxy,
            notifications_proxy,
            connectivity_proxy,
            device_signals,
            notifications_signals,
            battery_refreshed,
            connectivity_refreshed,
        }))
    }

    async fn wait_for_change(&mut self) {
        select! {
            _ = self.device_signals.next() => (),
            _ = self.notifications_signals.next() => (),
            _ = self.battery_refreshed.next() => (),
            _ = self.connectivity_refreshed.next() => (),
        }
    }

    async fn connected(&self) -> bool {
        self.device_proxy.is_reachable().await.unwrap_or(false)
    }

    async fn name(&self) -> Option<String> {
        self.device_proxy.name().await.ok()
    }

    async fn battery(&self) -> (Option<u8>, bool) {
        let (charge, is_charging) = tokio::join!(
            self.battery_proxy.charge(),
            self.battery_proxy.is_charging(),
        );
        (
            charge.ok().map(|x| x.clamp(0, 100) as u8),
            is_charging.unwrap_or(false),
        )
    }

    async fn notifications(&self) -> usize {
        self.notifications_proxy
            .active_notifications()
            .await
            .map(|n| n.len())
            .unwrap_or(0)
    }

    async fn network(&self) -> (Option<String>, i32) {
        let (ty, strength) = tokio::join!(
            self.connectivity_proxy.cellular_network_type(),
            self.connectivity_proxy.cellular_network_strength(),
        );
        (ty.ok(), strength.unwrap_or(-1))
    }
}

#[zbus::proxy(
    interface = "org.kde.kdeconnect.daemon",
    default_service = "org.kde.kdeconnect",
    default_path = "/modules/kdeconnect"
)]
trait DaemonDbus {
    #[zbus(name = "devices")]
    fn devices(&self) -> zbus::Result<Vec<String>>;

    #[zbus(signal, name = "deviceAdded")]
    fn device_added(&self, id: String) -> zbus::Result<()>;

    #[zbus(signal, name = "deviceRemoved")]
    fn device_removed(&self, id: String) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.kde.kdeconnect.device",
    default_service = "org.kde.kdeconnect"
)]
trait DeviceDbus {
    #[zbus(property, name = "isReachable")]
    fn is_reachable(&self) -> zbus::Result<bool>;

    #[zbus(signal, name = "reachableChanged")]
    fn reachable_changed(&self, reachable: bool) -> zbus::Result<()>;

    #[zbus(property, name = "name")]
    fn name(&self) -> zbus::Result<String>;

    #[zbus(signal, name = "nameChanged")]
    fn name_changed_(&self, name: &str) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.kde.kdeconnect.device.notifications",
    default_service = "org.kde.kdeconnect"
)]
trait NotificationsDbus {
    #[zbus(name = "activeNotifications")]
    fn active_notifications(&self) -> zbus::Result<Vec<String>>;

    #[zbus(signal, name = "allNotificationsRemoved")]
    fn all_notifications_removed(&self) -> zbus::Result<()>;

    #[zbus(signal, name = "notificationPosted")]
    fn notification_posted(&self, id: &str) -> zbus::Result<()>;

    #[zbus(signal, name = "notificationRemoved")]
    fn notification_removed(&self, id: &str) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_declares_connection_states_and_icons() {
        let plan = prepare(&Config::default()).unwrap();
        let ids: Vec<_> = plan.outputs.iter().map(|o| o.id).collect();
        assert_eq!(ids, ["connected", "disconnected", "missing"]);

        let connected = plan.output("connected").unwrap();
        assert_eq!(connected.single_icon("icon").unwrap(), "phone");
        let bat = connected.output().choices_for("bat_icon").unwrap();
        assert!(bat.permits("bat") && bat.permits("bat_charging"));
        assert!(!bat.permits("phone"));

        for id in ["disconnected", "missing"] {
            let output = plan.output(id).unwrap();
            assert_eq!(output.single_icon("icon").unwrap(), "phone_disconnected");
        }
        // `missing` renders only `$icon`; the battery/network/notification
        // values are never set in that state.
        assert!(
            plan.output("missing")
                .unwrap()
                .output()
                .choices_for("bat_icon")
                .is_none()
        );
    }
}
