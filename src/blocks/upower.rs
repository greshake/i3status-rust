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
use blocks::dbus::{BusType, Connection, Message};

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
            // TODO: use correct icons.
            BatteryState::Unknown => write!(f, "bat_full"),
            BatteryState::Charging => write!(f, "bat_full"),
            BatteryState::Discharging => write!(f, "bat_full"),
            BatteryState::Empty => write!(f, "bat_full"),
            BatteryState::FullyCharged => write!(f, "bat_full"),
            BatteryState::PendingCharge => write!(f, "bat_full"),
            BatteryState::PendingDischarge => write!(f, "bat_full"),
        }
    }
}

struct Battery {
    device: String,
}


/*
let (rotated, next) = if self.marquee {
    self.current_song.next()?
} else {
    (false, None)
};

if !rotated {
    let c = self.dbus_conn.with_path(
        format!("org.mpris.MediaPlayer2.{}", self.player),
        "/org/mpris/MediaPlayer2",
        1000,
    );
    let data = c.get("org.mpris.MediaPlayer2.Player", "Metadata");

    if data.is_err() {
        self.current_song.set_text(String::from(""));
        self.player_avail = false;
    } else {

        if title.is_empty() && artist.is_empty() {
            self.player_avail = false;
            self.current_song.set_text(String::new());
        } else {
            self.player_avail = true;
            self.current_song
                .set_text(format!("{} | {}", title, artist));
        }
    }
}
*/
impl Battery {
    pub fn default(c: &Connection) -> Result<Self> {
        // TODO: fetch all available devices from upower.
        let devices: Vec<String> = Vec::new();

        match devices.into_iter().find(|x| Self::is_valid_device(c, x)) {
            Some(device) => return Ok(Battery {
                device: device,
            }),
            None => return Err(BlockError(
                "upower".to_string(),
                "No valid device found".to_string(),
            )),
        }
    }

    pub fn from(c: &Connection, device: String) -> Result<Self> {
        match Self::is_valid_device(c, &device) {
            true => Ok(Battery {
                device: device,
            }),
            false => Err(BlockError(
                "upower".to_string(),
                "Device is invalid".to_string(),
            )),
        }
    }

    pub fn get_device(&self) -> String {
        self.device.clone()
    }

    pub fn build_device_path(device: &str) -> String {
        format!("/org/freedesktop/UPower/devices/{}", device)
    }

    fn get_property(c: &Connection, device: &str, property: &str) -> Result<Message> {
        let m = Message::new_method_call(
            "org.freedesktop.UPower",
            Self::build_device_path(device),
            "org.freedesktop.UPower.Device",
            property)
            .block_error("upower", "Failed to create message")?;

        let r = c.send_with_reply_and_block(m, 1000);

        // TODO: find more elegant solution.
        match r {
            Ok(msg) => Ok(msg),
            Err(_) => Err(BlockError(
                "upower".to_string(),
                "Failed to retrieve property".to_string(),
            )),
        }
    }

    fn is_valid_device(c: &Connection, device: &str) -> bool {
        let m = Self::get_property(c, device, "Type");

        match m {
            Err(_) => false,
            Ok(msg) => {
                let is_present: Option<bool> = msg.get1();
                match is_present {
                    None => false,
                    Some(val) => val,
                }
            },
        }
    }

    pub fn percentage(&self, c: &Connection) -> Result<Option<u8>> {
        let m = try!(Self::get_property(c, &self.get_device(), "IsPresent"));
        let is_present: bool = m.get1().block_error(
            "upower", "Failed to read property"
        )?;

        if !is_present {
            return Ok(None)
        }

        let m = try!(Self::get_property(c, &self.get_device(), "Percentage"));
        let percentage: u8 = m.get1().block_error(
            "upower", "Failed to read property"
        )?;

        Ok(Some(percentage))
    }

    pub fn state(&self, c: &Connection) -> Result<BatteryState> {
        let m = try!(Self::get_property(c, &self.get_device(), "State"));

        let state: u32 = m.get1().block_error(
            "upower", "Failed to read property"
        )?;

        Ok(BatteryState::from(state))
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
        let device_path = Battery::build_device_path(&battery.get_device());

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
            Some(p) => format!("{:02}%", p),
            None => "-".to_string(),
        };

        self.output.set_text(text);
        self.output.set_icon(&state.to_string());
        self.output.set_state(match percentage {
            Some(0...20) => State::Good,
            Some(21...45) => State::Idle,
            Some(46...60) => State::Info,
            Some(61...80) => State::Warning,
            _ => State::Critical,
        });

        Ok(None)
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.output]
    }
}
