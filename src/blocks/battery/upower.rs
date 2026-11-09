use tokio::try_join;
use zbus::fdo::{PropertiesChangedStream, PropertiesProxy};
use zbus::proxy::{CacheProperties, OwnerChangedStream};
use zbus::{Connection, zvariant};
use zvariant::ObjectPath;

use super::{BatteryDevice, BatteryInfo, BatteryStatus, DeviceName};
use crate::blocks::prelude::*;
use crate::util::new_system_dbus_connection;

make_log_macro!(debug, "battery[upower]");

const DISPLAY_DEVICE_PATH: ObjectPath =
    ObjectPath::from_static_str_unchecked("/org/freedesktop/UPower/devices/DisplayDevice");

struct DeviceConnection {
    device_proxy: DeviceProxy<'static>,
    changes: PropertiesChangedStream,
}

impl DeviceConnection {
    async fn new(
        dbus_conn: &Connection,
        upower_proxy: &UPowerProxy<'_>,
        device: &DeviceName,
        expected_model: Option<&str>,
    ) -> Result<Option<Self>> {
        let device_proxy =
            if device.exact().is_none_or(|d| d == "DisplayDevice") && expected_model.is_none() {
                device_proxy(dbus_conn, DISPLAY_DEVICE_PATH).await?
            } else {
                let mut res = None;
                for path in upower_proxy
                    .enumerate_devices()
                    .await
                    .error("Failed to retrieve UPower devices")?
                {
                    let proxy = device_proxy(dbus_conn, path).await?;
                    match device_matches(&proxy, device, expected_model).await {
                        Ok(true) => {
                            res = Some(proxy);
                            break;
                        }
                        Ok(false) => (),
                        // Most likely removed since it was listed
                        Err(err) => {
                            debug!("skipping {}: {err}", proxy.inner().path());
                        }
                    }
                }
                match res {
                    Some(res) => res,
                    None => return Ok(None),
                }
            };

        let changes = PropertiesProxy::builder(dbus_conn)
            .destination("org.freedesktop.UPower")
            .unwrap()
            .path(device_proxy.inner().path().to_owned())
            .unwrap()
            .build()
            .await
            .error("Failed to create PropertiesProxy")?
            .receive_properties_changed()
            .await
            .error("Failed to create PropertiesChangedStream")?;

        Ok(Some(DeviceConnection {
            device_proxy,
            changes,
        }))
    }
}

/// Whether a device listed by UPower is the configured one.
async fn device_matches(
    proxy: &DeviceProxy<'_>,
    device: &DeviceName,
    expected_model: Option<&str>,
) -> zbus::Result<bool> {
    // Filter by model if needed
    if let Some(expected_model) = expected_model
        && let Ok(device_model) = proxy.model().await
        && expected_model != device_model
    {
        return Ok(false);
    }
    // Verify device type
    // https://upower.freedesktop.org/docs/Device.html#Device:Type
    // consider any peripheral, UPS and internal battery
    if proxy.type_().await? == 1 {
        return Ok(false);
    }
    Ok(device.matches(&proxy.native_path().await?))
}

/// The properties are read from UPower each time rather than from zbus' cache. The cache is
/// filled once, when the proxy is built, and stays unusable if UPower could not be reached then.
/// It is also kept up to date by a task of its own, which may not have applied a change yet when
/// the block wakes up for it.
async fn device_proxy(
    dbus_conn: &Connection,
    path: impl Into<ObjectPath<'static>>,
) -> Result<DeviceProxy<'static>> {
    DeviceProxy::builder(dbus_conn)
        .path(path.into())
        .unwrap()
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .error("Failed to create DeviceProxy")
}

pub(super) struct Device {
    dbus_conn: Connection,
    upower_proxy: UPowerProxy<'static>,
    device: DeviceName,
    dev_model: Option<String>,
    device_conn: Option<DeviceConnection>,
    owner_changed_stream: OwnerChangedStream<'static>,
    device_added_stream: DeviceAddedStream,
    device_removed_stream: DeviceRemovedStream,
}

impl Device {
    pub(super) async fn new(device: DeviceName, dev_model: Option<String>) -> Result<Self> {
        let dbus_conn = new_system_dbus_connection().await?;

        let upower_proxy = UPowerProxy::new(&dbus_conn)
            .await
            .error("Could not create UPowerProxy")?;

        // Subscribe before looking for the device, so that no change in between is missed. UPower
        // does not have to be running yet: the streams follow its bus name.
        let (owner_changed_stream, device_added_stream, device_removed_stream) = try_join! {
            upower_proxy.inner().receive_owner_changed(),
            upower_proxy.receive_device_added(),
            upower_proxy.receive_device_removed()
        }
        .error("Could not create signal stream")?;

        let mut this = Self {
            dbus_conn,
            upower_proxy,
            device,
            dev_model,
            device_conn: None,
            owner_changed_stream,
            device_added_stream,
            device_removed_stream,
        };
        this.device_conn = this.find_device().await;
        Ok(this)
    }

    /// Look for the configured device.
    ///
    /// UPower may stop or restart at any time, so failing to talk to it means that there is no
    /// device for now. The block looks again once UPower reappears or reports a new device.
    async fn find_device(&self) -> Option<DeviceConnection> {
        DeviceConnection::new(
            &self.dbus_conn,
            &self.upower_proxy,
            &self.device,
            self.dev_model.as_deref(),
        )
        .await
        .unwrap_or_else(|err| {
            debug!("could not find the device: {err}");
            None
        })
    }

    /// UPower stopped, started or was replaced. Nothing from the previous instance is valid
    /// anymore, including the paths of its devices.
    async fn on_owner_changed(&mut self, running: bool) {
        self.device_conn = if running {
            debug!("UPower appeared on the bus");
            self.find_device().await
        } else {
            debug!("UPower left the bus");
            None
        };
    }
}

#[async_trait]
impl BatteryDevice for Device {
    async fn get_info(&mut self) -> Result<Option<BatteryInfo>> {
        match &self.device_conn {
            None => Ok(None),
            Some(device_conn) => {
                match try_join! {
                    device_conn.device_proxy.is_present(),
                    device_conn.device_proxy.percentage(),
                    device_conn.device_proxy.energy_rate(),
                    device_conn.device_proxy.state(),
                    device_conn.device_proxy.time_to_full(),
                    device_conn.device_proxy.time_to_empty(),
                } {
                    // The display device keeps existing without any battery behind it, and
                    // hot-removable batteries are listed while their bay is empty.
                    Err(_) | Ok((false, ..)) => Ok(None),
                    Ok((true, capacity, power, state, time_to_full, time_to_empty)) => {
                        let status = match state {
                            1 => BatteryStatus::Charging,
                            2 | 6 => BatteryStatus::Discharging,
                            3 => BatteryStatus::Empty,
                            4 => BatteryStatus::Full,
                            5 => BatteryStatus::NotCharging,
                            _ => BatteryStatus::Unknown,
                        };

                        let time_remaining = match status {
                            BatteryStatus::Charging => Some(time_to_full as f64),
                            BatteryStatus::Discharging => Some(time_to_empty as f64),
                            _ => None,
                        };

                        Ok(Some(BatteryInfo {
                            status,
                            capacity,
                            power: Some(power),
                            time_remaining,
                        }))
                    }
                }
            }
        }
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        // The streams only end when the connection to the bus is lost. Returning an error then
        // restarts the block, instead of polling ended streams in a busy loop.
        loop {
            match &mut self.device_conn {
                Some(device_conn) => select! {
                    Some(_) = device_conn.changes.next() => break,
                    Some(_) = self.device_added_stream.next() => (),
                    Some(msg) = self.device_removed_stream.next() => {
                        let args = msg.args().error("Failed to get the args")?;
                        if args.device().as_ref() == device_conn.device_proxy.inner().path().as_ref() {
                            // Another device may match as well
                            self.device_conn = self.find_device().await;
                            break;
                        }
                    },
                    Some(owner) = self.owner_changed_stream.next() => {
                        self.on_owner_changed(owner.is_some()).await;
                        break;
                    },
                    else => return Err(Error::new("Lost the connection to the system bus")),
                },
                None => select! {
                    Some(_) = self.device_removed_stream.next() => (),
                    Some(_) = self.device_added_stream.next() => {
                        if let Some(device_conn) = self.find_device().await {
                            self.device_conn = Some(device_conn);
                            break;
                        }
                    },
                    Some(owner) = self.owner_changed_stream.next() => {
                        self.on_owner_changed(owner.is_some()).await;
                        break;
                    },
                    else => return Err(Error::new("Lost the connection to the system bus")),
                },
            }
        }

        Ok(())
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower"
)]
trait Device {
    #[zbus(property)]
    fn energy_rate(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn native_path(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn model(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn online(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;

    #[zbus(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;

    #[zbus(property, name = "Type")]
    fn type_(&self) -> zbus::Result<u32>;
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
trait UPower {
    fn enumerate_devices(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    fn get_display_device(&self) -> zbus::Result<zvariant::OwnedObjectPath>;

    #[zbus(signal)]
    fn device_added(&self, device: zvariant::OwnedObjectPath) -> zbus::Result<()>;

    #[zbus(signal)]
    fn device_removed(&self, device: zvariant::OwnedObjectPath) -> zbus::Result<()>;

    #[zbus(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
}
