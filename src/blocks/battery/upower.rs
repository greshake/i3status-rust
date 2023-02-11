use tokio::try_join;
use zbus::fdo::{PropertiesChangedStream, PropertiesProxy};
use zbus::{zvariant, Connection};
use zvariant::ObjectPath;

use super::{BatteryDevice, BatteryInfo, BatteryStatus, DeviceName};
use crate::blocks::prelude::*;
use crate::util::new_system_dbus_connection;

struct DeviceConnection {
    device_path: ObjectPath<'static>,
    device_proxy: DeviceProxy<'static>,
    changes: PropertiesChangedStream<'static>,
}

impl DeviceConnection {
    async fn new(dbus_conn: &Connection, device: &DeviceName) -> Result<Option<Self>> {
        let device_conn_info = if device.exact() == Some("DisplayDevice") {
            let path: ObjectPath = "/org/freedesktop/UPower/devices/DisplayDevice"
                .try_into()
                .unwrap();
            let proxy = DeviceProxy::builder(dbus_conn)
                .path(path.clone())
                .unwrap()
                .build()
                .await
                .error("Failed to create DeviceProxy")?;
            Some((path, proxy))
        } else {
            let mut res = None;
            for path in UPowerProxy::new(dbus_conn)
                .await
                .error("Failed to create UPowerProxy")?
                .enumerate_devices()
                .await
                .error("Failed to retrieve UPower devices")?
            {
                let proxy = DeviceProxy::builder(dbus_conn)
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
            res
        };

        Ok(match device_conn_info {
            Some((device_path, device_proxy)) => {
                let changes = PropertiesProxy::builder(dbus_conn)
                    .destination("org.freedesktop.UPower")
                    .and_then(|x| x.path(device_path.clone()))
                    .unwrap()
                    .build()
                    .await
                    .error("Failed to create PropertiesProxy")?
                    .receive_properties_changed()
                    .await
                    .error("Failed to create PropertiesChangedStream")?;
                Some(DeviceConnection {
                    device_path,
                    device_proxy,
                    changes,
                })
            }
            None => None,
        })
    }
}

pub(super) struct Device {
    dbus_conn: Connection,
    device: DeviceName,
    device_conn: Option<DeviceConnection>,
    device_added_stream: DeviceAddedStream<'static>,
    device_removed_stream: DeviceRemovedStream<'static>,
}

impl Device {
    pub(super) async fn new(device: DeviceName) -> Result<Self> {
        let dbus_conn = new_system_dbus_connection().await?;

        let device_conn = DeviceConnection::new(&dbus_conn, &device).await?;

        let upower_proxy = UPowerProxy::new(&dbus_conn)
            .await
            .error("Could not create UPowerProxy")?;

        let (device_added_stream, device_removed_stream) = try_join! {
            upower_proxy.receive_device_added(),
            upower_proxy.receive_device_removed()
        }
        .error("Could not create signal stream")?;

        Ok(Self {
            dbus_conn,
            device,
            device_conn,
            device_added_stream,
            device_removed_stream,
        })
    }
}

#[async_trait]
impl BatteryDevice for Device {
    async fn get_info(&mut self) -> Result<Option<BatteryInfo>> {
        match &self.device_conn {
            None => Ok(None),
            Some(device_conn) => {
                match try_join! {
                    device_conn.device_proxy.percentage(),
                    device_conn.device_proxy.energy_rate(),
                    device_conn.device_proxy.state(),
                    device_conn.device_proxy.time_to_full(),
                    device_conn.device_proxy.time_to_empty(),
                } {
                    Err(_) => Ok(None),
                    Ok((capacity, power, state, time_to_full, time_to_empty)) => {
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
        match &mut self.device_conn {
            Some(device_conn) => loop {
                select! {
                    _ = self.device_added_stream.next() => {},
                    _ = device_conn.changes.next() => {
                        break;
                    },
                    Some(msg) = self.device_removed_stream.next() => {
                        let args = msg.args().unwrap();
                        if args.device().as_ref() == device_conn.device_path {
                            self.device_conn = None;
                            break;
                        }
                    },
                }
            },
            None => loop {
                select! {
                    _ = self.device_removed_stream.next() => {},
                    _ = self.device_added_stream.next() => {
                        if let Some(device_conn) =
                        DeviceConnection::new(&self.dbus_conn, &self.device).await?
                        {
                            self.device_conn = Some(device_conn);
                            break;
                        }
                    },
                }
            },
        }

        Ok(())
    }
}

#[zbus::dbus_proxy(
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

#[zbus::dbus_proxy(
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
