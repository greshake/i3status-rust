use std::fmt;
use std::time::{Duration, Instant};
use std::thread;

use chan::Sender;
use uuid::Uuid;

use config::Config;
use errors::*;
use scheduler::Task;
use block::{Block, ConfigBlock};
use widget::{I3BarWidget, State};
use widgets::text::TextWidget;
use blocks::dbus::{BusType, Connection, Message, MessageItem, Path};
use blocks::dbus::arg::Variant;

#[derive(Clone, Copy)]
enum DeviceType {
    Unknown,
    LinePower,
    Battery,
    Ups,
    Monitor,
    Mouse,
    Keyboard,
    Pda,
    Phone,
}

impl DeviceType {
    pub fn to_u32(&self) -> u32 {
        *self as u32
    }
}

impl From<u32> for DeviceType {
    fn from(id: u32) -> Self {
        match id {
            // https://upower.freedesktop.org/docs/Device.html#Device:Type
            // TODO: derive this automatically.
            1 => DeviceType::LinePower,
            2 => DeviceType::Battery,
            3 => DeviceType::Ups,
            4 => DeviceType::Monitor,
            5 => DeviceType::Mouse,
            6 => DeviceType::Keyboard,
            7 => DeviceType::Pda,
            8 => DeviceType::Phone,
            _ => DeviceType::Unknown,
        }
    }
}

enum BatteryState {
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
}

impl From<u32> for BatteryState {
    fn from(id: u32) -> Self {
        match id {
            // https://upower.freedesktop.org/docs/Device.html#Device:State
            // TODO: derive this automatically.
            1 => BatteryState::Charging,
            2 => BatteryState::Discharging,
            3 => BatteryState::Empty,
            4 => BatteryState::FullyCharged,
            5 => BatteryState::PendingCharge,
            6 => BatteryState::PendingDischarge,
            _ => BatteryState::Unknown,
        }
    }
}

impl fmt::Display for BatteryState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BatteryState::Unknown => write!(f, "bat"),
            BatteryState::Charging => write!(f, "bat_charging"),
            BatteryState::Discharging => write!(f, "bat_discharging"),
            BatteryState::Empty => write!(f, "bat"),
            BatteryState::FullyCharged => write!(f, "bat_full"),
            BatteryState::PendingCharge => write!(f, "bat_charging"),
            BatteryState::PendingDischarge => write!(f, "bat_discharging"),
        }
    }
}

struct Battery {
    device: String,
}

impl Battery {
    pub fn default(c: &Connection) -> Result<Self> {
        let m = Message::new_method_call(
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower",
            "org.freedesktop.UPower",
            "EnumerateDevices")
            .block_error("upower", "Failed to create message")?;

        let r = c.send_with_reply_and_block(m, 1000)
            .block_error("upower", "Failed to retrieve property")?;

        let devices: Vec<Path> = r.get1()
            .block_error("upower", "Failed to read property")?;

        match devices.into_iter().find(|x| Self::is_valid_device(c, x)) {
            Some(device) => return Ok(Battery {
                device: device.split("/").last().unwrap().to_string(),
            }),
            None => return Err(BlockError(
                "upower".to_string(),
                "No valid device found".to_string(),
            )),
        }
    }

    pub fn from(c: &Connection, device: String) -> Result<Self> {
        let device_path = Self::build_device_path(&device);
        match Self::is_valid_device(c, &device_path) {
            true => Ok(Battery {
                device: device,
            }),
            false => Err(BlockError(
                "upower".to_string(),
                "Device is invalid".to_string(),
            )),
        }
    }

    pub fn get_device_path(&self) -> String {
        format!("/org/freedesktop/UPower/devices/{}", self.device)
    }

    pub fn build_device_path(device: &str) -> String {
        format!("/org/freedesktop/UPower/devices/{}", device)
    }

    fn get_property(c: &Connection, device_path: &str, property: &str) -> Result<Message> {
        let m = Message::new_method_call(
            "org.freedesktop.UPower",
            device_path,
            "org.freedesktop.DBus.Properties",
            "Get")
            .block_error("upower", "Failed to create message")?
            .append2(
                MessageItem::Str("org.freedesktop.UPower.Device".to_string()),
                MessageItem::Str(property.to_string())
            );

        let r = c.send_with_reply_and_block(m, 1000);
        r.block_error("upower", "Failed to retrieve property")
    }

    fn is_valid_device(c: &Connection, device_path: &str) -> bool {
        let m = Self::get_property(c, device_path, "Type");

        match m {
            Err(_) => false,
            Ok(msg) => {
                let type_battery = DeviceType::Battery.to_u32();
                let type_: Option<Variant<u32>> = msg.get1();

                match type_ {
                    Some(Variant(m)) if m == type_battery => true,
                    _ => false,
                }
            },
        }
    }

    pub fn percentage(&self, c: &Connection) -> Result<Option<u8>> {
        let m = try!(Self::get_property(c, &self.get_device_path(), "IsPresent"));
        let is_present: Variant<bool> = m.get1().block_error(
            "upower", "Failed to read property"
        )?;

        if !is_present.0 {
            return Ok(None)
        }

        let m = try!(Self::get_property(c, &self.get_device_path(), "Percentage"));
        let percentage: Variant<f64> = m.get1().block_error(
            "upower", "Failed to read property"
        )?;

        Ok(Some(percentage.0 as u8))
    }

    pub fn state(&self, c: &Connection) -> Result<BatteryState> {
        let m = try!(Self::get_property(c, &self.get_device_path(), "State"));

        let state: Variant<u32> = m.get1().block_error(
            "upower", "Failed to read property"
        )?;

        Ok(BatteryState::from(state.0))
    }
}

pub struct Upower {
    id: String,
    output: TextWidget,
    dbus_conn: Connection,
    battery: Battery,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct UpowerConfig {
    /// Name of the power device to use.
    #[serde(default = "UpowerConfig::default_device")]
    pub device: Option<String>,
}

impl UpowerConfig {
    fn default_device() -> Option<String> {
        None
    }
}

impl ConfigBlock for Upower {
    type Config = UpowerConfig;

    fn new(block_config: Self::Config, config: Config, send: Sender<Task>) -> Result<Self> {
        let id: String = Uuid::new_v4().simple().to_string();
        let id_copy = id.clone();
        let dbus_conn = Connection::get_private(BusType::System)
            .block_error("upower", "failed to establish D-Bus connection")?;
        let battery = try!(match block_config.device {
            Some(device) => Battery::from(&dbus_conn, device),
            None => Battery::default(&dbus_conn),
        });
        let device_path = battery.get_device_path();

        thread::spawn(move || {
            let c = Connection::get_private(BusType::System).unwrap();
            let rule = format!(
                "type='signal',\
                 path='{}',\
                 interface='org.freedesktop.DBus.Properties',\
                 member='PropertiesChanged'",
                device_path);

            c.add_match(&rule).unwrap();

            loop {
                let timeout = 100000;

                for _event in c.iter(timeout) {
                    send.send(Task {
                        id: id.clone(),
                        update_time: Instant::now(),
                    });
                }
            }
        });

        let state = try!(battery.state(&dbus_conn));

        Ok(Upower {
            id: id_copy,
            output: TextWidget::new(config).with_icon(&state.to_string()),
            dbus_conn: dbus_conn,
            battery: battery,
        })
    }
}

impl Block for Upower {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Result<Option<Duration>> {
        let state = self.battery.state(&self.dbus_conn)?;
        let percentage = self.battery.percentage(&self.dbus_conn)?;
        let text = match percentage {
            p @ Some(0...100) => format!("{:02}%", p.unwrap()),
            _ => "-".to_string(),
        };

        self.output.set_text(text);
        self.output.set_icon(&state.to_string());
        self.output.set_state(match percentage {
            Some(0...20) => State::Critical,
            Some(21...45) => State::Warning,
            Some(46...60) => State::Idle,
            Some(61...80) => State::Info,
            Some(81...100) => State::Good,
            _ => State::Critical,
        });

        Ok(None)
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }
}
