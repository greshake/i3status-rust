use zbus::dbus_proxy;
use zbus::fdo::DBusProxy;
use zbus::MessageStream;
use zvariant::ObjectPath;

use super::{BatteryDevice, BatteryInfo, BatteryStatus, DeviceName};
use crate::blocks::prelude::*;
use crate::util::new_system_dbus_connection;

pub(super) struct Device {
    device_proxy: DeviceProxy<'static>,
    changes: MessageStream,
}

impl Device {
    pub(super) async fn new(device: DeviceName) -> Result<Self> {
        let dbus_conn = new_system_dbus_connection().await?;
        let (device_path, device_proxy) = {
            if device.exact() == Some("DisplayDevice") {
                let path: ObjectPath = "/org/freedesktop/UPower/devices/DisplayDevice"
                    .try_into()
                    .unwrap();
                let proxy = DeviceProxy::builder(&dbus_conn)
                    .path(path.clone())
                    .unwrap()
                    .build()
                    .await
                    .error("Failed to create DeviceProxy")?;
                (path, proxy)
            } else {
                let mut res = None;
                for path in UPowerProxy::new(&dbus_conn)
                    .await
                    .error("Failed to create UPwerProxy")?
                    .enumerate_devices()
                    .await
                    .error("Failed to retrieve UPower devices")?
                {
                    let proxy = DeviceProxy::builder(&dbus_conn)
                        .path(path.clone())
                        .unwrap()
                        .build()
                        .await
                        .error("Failed to create DeviceProxy")?;
                    // Verify device type
                    // https://upower.freedesktop.org/docs/Device.html#Device:Type
                    // consider any peripheral, UPS and internal battery
                    let device_type = proxy.type_().await.error("Failed to get device's type")?;
                    if device_type == 1 {
                        continue;
                    }
                    let name = proxy
                        .native_path()
                        .await
                        .error("Failed to get device's native path")?;
                    if device.matches(&name) {
                        res = Some((path.into_inner(), proxy));
                        break;
                    }
                }
                match res {
                    Some(res) => res,
                    // FIXME
                    None => return Err(Error::new("UPower device could not be found")),
                }
            }
        };

        let dbus_conn = new_system_dbus_connection().await?;
        DBusProxy::new(&dbus_conn)
            .await
            .error("failed to cerate DBusProxy")?
            .add_match(&format!("type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',path='{}'", device_path.as_str()))
            .await
            .error("Failed to add match")?;
        let changes = MessageStream::from(dbus_conn);

        Ok(Self {
            device_proxy,
            changes,
        })
    }
}

#[async_trait]
impl BatteryDevice for Device {
    async fn get_info(&mut self) -> Result<Option<BatteryInfo>> {
        let capacity = self
            .device_proxy
            .percentage()
            .await
            .error("Failed to get capacity")?;

        let power = self
            .device_proxy
            .energy_rate()
            .await
            .error("Failed to get power")?;

        let status = match self
            .device_proxy
            .state()
            .await
            .error("Failed to get status")?
        {
            1 => BatteryStatus::Charging,
            2 | 6 => BatteryStatus::Discharging,
            3 => BatteryStatus::Empty,
            4 => BatteryStatus::Full,
            5 => BatteryStatus::NotCharging,
            _ => BatteryStatus::Unknown,
        };

        let time_remaining = match status {
            BatteryStatus::Charging => Some(
                self.device_proxy
                    .time_to_full()
                    .await
                    .error("Failed to get time to full")? as f64,
            ),
            BatteryStatus::Discharging => Some(
                self.device_proxy
                    .time_to_empty()
                    .await
                    .error("Failed to get time to empty")? as f64,
            ),
            _ => None,
        };

        Ok(Some(BatteryInfo {
            status,
            capacity,
            power: Some(power),
            time_remaining,
        }))
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        self.changes.next().await;
        Ok(())
    }
}

#[dbus_proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower"
)]
trait Device {
    #[dbus_proxy(property)]
    fn energy_rate(&self) -> zbus::Result<f64>;

    #[dbus_proxy(property)]
    fn is_present(&self) -> zbus::Result<bool>;

    #[dbus_proxy(property)]
    fn native_path(&self) -> zbus::Result<String>;

    #[dbus_proxy(property)]
    fn online(&self) -> zbus::Result<bool>;

    #[dbus_proxy(property)]
    fn percentage(&self) -> zbus::Result<f64>;

    #[dbus_proxy(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[dbus_proxy(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;

    #[dbus_proxy(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;

    #[dbus_proxy(property, name = "Type")]
    fn type_(&self) -> zbus::Result<u32>;
}

#[dbus_proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
trait UPower {
    fn enumerate_devices(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    fn get_display_device(&self) -> zbus::Result<zvariant::OwnedObjectPath>;

    #[dbus_proxy(signal)]
    fn device_added(&self, device: zvariant::OwnedObjectPath) -> zbus::Result<()>;

    #[dbus_proxy(signal)]
    fn device_removed(&self, device: zvariant::OwnedObjectPath) -> zbus::Result<()>;

    #[dbus_proxy(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
}
