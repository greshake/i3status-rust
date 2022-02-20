use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::arg;
use dbus::blocking::{stdintf::org_freedesktop_dbus::Properties, Connection};
use dbus::Message;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::scheduler::Task;
use crate::util::battery_level_to_icon;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

pub struct KDEConnect {
    id: usize,
    device_id: String,
    device_name: Arc<Mutex<String>>,
    battery_charge: Arc<Mutex<i32>>,
    battery_state: Arc<Mutex<bool>>,
    notif_count: Arc<Mutex<i32>>,
    phone_reachable: Arc<Mutex<bool>>,
    // TODO
    //notif_text: Arc<Mutex<String>>,
    bat_good: i32,
    bat_info: i32,
    bat_warning: i32,
    bat_critical: i32,
    format: FormatTemplate,
    format_disconnected: FormatTemplate,
    output: TextWidget,
    shared_config: SharedConfig,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct KDEConnectConfig {
    pub device_id: Option<String>,

    /// The threshold above which the remaining capacity is shown as good
    pub bat_good: i32,

    /// The threshold below which the remaining capacity is shown as info
    pub bat_info: i32,

    /// The threshold below which the remaining capacity is shown as warning
    pub bat_warning: i32,

    /// The threshold below which the remaining capacity is shown as critical
    pub bat_critical: i32,

    /// Format string for displaying phone information.
    pub format: FormatTemplate,

    /// Format string for displaying phone information when it is disconnected.
    pub format_disconnected: FormatTemplate,
}

impl Default for KDEConnectConfig {
    fn default() -> Self {
        Self {
            device_id: None,
            bat_good: 60,
            bat_info: 60,
            bat_warning: 30,
            bat_critical: 15,
            format: FormatTemplate::default(),
            format_disconnected: FormatTemplate::default(),
        }
    }
}

impl ConfigBlock for KDEConnect {
    type Config = KDEConnectConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        send: Sender<Task>,
    ) -> Result<Self> {
        let send2 = send.clone();
        let send3 = send.clone();
        let send4 = send.clone();
        let send5 = send.clone();
        let send6 = send.clone();
        let send7 = send.clone();

        let c = Connection::new_session()
            .block_error("kdeconnect", "Failed to establish D-Bus connection")?;

        let device_id = if block_config.device_id.is_none() {
            // If none specified in block config, just grab the first device found.
            let p1 = c.with_proxy(
                "org.kde.kdeconnect",
                "/modules/kdeconnect",
                Duration::from_millis(5000),
            );
            // method call opts: only_reachable=false, only_paired=true
            let (devices,): (Vec<String>,) = p1
                .method_call("org.kde.kdeconnect.daemon", "devices", (false, true))
                .block_error("kdeconnect", "Couldn't connect to KDE Connect daemon")?;
            if devices.is_empty() {
                return Err(BlockError(
                    "kdeconnect".to_owned(),
                    "No devices found.".to_owned(),
                ));
            }
            devices[0].clone()
        } else {
            block_config.device_id.unwrap()
        };

        let p2 = c.with_proxy(
            "org.kde.kdeconnect",
            format!("/modules/kdeconnect/devices/{}", device_id),
            Duration::from_millis(5000),
        );
        let initial_name: String = p2
            .get("org.kde.kdeconnect.device", "name")
            .unwrap_or_else(|_| String::from(""));
        let initial_reachable: bool = p2
            .get("org.kde.kdeconnect.device", "isReachable")
            .unwrap_or(false);

        // Test whether we are dealing with kdeconnect v20.08.03 or older,
        // or kdeconnect v20.11.80 or newer, so we can adapt to the differences.
        //
        // Starting with kdeconnect v20.11.80, the version output by the cli
        // matches the versioning scheme used by Ubuntu, where as before that it
        // was  1.3.x or 1.4.x.
        let old_kdeconnect = Command::new("kdeconnect-cli")
            .args(&["--version"])
            .output()
            .block_error(
                "kdeconnect",
                "Failed to check kdeconnect version. Is it installed?",
            )
            .and_then(|raw_output| {
                String::from_utf8(raw_output.stdout)
                    .block_error("kdeconnect", "Failed to check kdeconnect version.")
            })
            .unwrap()
            .contains("kdeconnect-cli 1.");

        let initial_charge = if old_kdeconnect {
            let (charge,): (i32,) = p2
                .method_call("org.kde.kdeconnect.device.battery", "charge", ())
                .unwrap_or((0,));
            charge
        } else {
            let p3 = c.with_proxy(
                "org.kde.kdeconnect",
                format!("/modules/kdeconnect/devices/{}/battery", device_id),
                Duration::from_millis(5000),
            );
            let charge: i32 = p3
                .get("org.kde.kdeconnect.device.battery", "charge")
                .unwrap_or(0);
            charge
        };

        let initial_charging = if old_kdeconnect {
            let (charging,): (bool,) = p2
                .method_call("org.kde.kdeconnect.device.battery", "isCharging", ())
                .unwrap_or((false,));
            charging
        } else {
            let p3 = c.with_proxy(
                "org.kde.kdeconnect",
                format!("/modules/kdeconnect/devices/{}/battery", device_id),
                Duration::from_millis(5000),
            );
            let charging: bool = p3
                .get("org.kde.kdeconnect.device.battery", "isCharging")
                .unwrap_or(false);
            charging
        };

        let initial_notifications = if old_kdeconnect {
            let (notifications,): (Vec<String>,) = p2
                .method_call(
                    "org.kde.kdeconnect.device.notifications",
                    "activeNotifications",
                    (),
                )
                .unwrap_or((vec![String::from("")],));
            notifications
        } else {
            let p4 = c.with_proxy(
                "org.kde.kdeconnect",
                format!("/modules/kdeconnect/devices/{}/notifications", device_id),
                Duration::from_millis(5000),
            );
            let (notifications,): (Vec<String>,) = p4
                .method_call(
                    "org.kde.kdeconnect.device.notifications",
                    "activeNotifications",
                    (),
                )
                .unwrap_or((vec![String::from("")],));
            notifications
        };

        let device_id_copy = device_id.clone();
        let device_name = Arc::new(Mutex::new(initial_name));
        let device_name_copy = device_name.clone();
        let charge = Arc::new(Mutex::new(initial_charge));
        let charge_copy = charge.clone();
        // TODO: revisit this lint
        #[allow(clippy::mutex_atomic)]
        let charging = Arc::new(Mutex::new(initial_charging));
        let charging_copy = charging.clone();
        let notif_count = Arc::new(Mutex::new(initial_notifications.len() as i32));
        let notif_count_copy1 = notif_count.clone();
        let notif_count_copy2 = notif_count.clone();
        let notif_count_copy3 = notif_count.clone();
        // TODO: revisit this lint
        #[allow(clippy::mutex_atomic)]
        let reachable = Arc::new(Mutex::new(initial_reachable));
        let reachable_copy1 = reachable.clone();
        let reachable_copy2 = reachable.clone();

        // TODO: See if can reliably get the text and/or app of the most recent notification.
        // Will need to see if the order of notifications is guaranteed or not.
        // Also, need to call activeNotifications each time a notification is added/removed/updated,
        // because the signal only gives us a 'public_id' and no other useful info
        //let last_notif_text = if initial_notifications.get(0).is_none() {
        //    Arc::new(Mutex::new("".to_string()))
        //} else {
        //    Arc::new(Mutex::new(initial_notifications.get(0).unwrap().to_string()))
        //};

        thread::Builder::new()
            .name("kdeconnect".into())
            .spawn(move || {
                let c = Connection::new_session()
                    .expect("Failed to establish D-Bus connection in thread");

                let p1 = c.with_proxy(
                    "org.kde.kdeconnect",
                    format!("/modules/kdeconnect/devices/{}", device_id_copy),
                    Duration::from_millis(5000),
                );

                let _device_name_handler = p1.match_signal(
                    move |s: OrgKdeKdeconnectDeviceNameChanged, _: &Connection, _: &Message| {
                        let mut name = device_name_copy.lock().unwrap();
                        *name = s.name;

                        // Tell block to update now.
                        send2
                            .send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap();

                        true
                    },
                );

                let _phone_reachable_handler = p1.match_signal(
                    move |s: OrgKdeKdeconnectDeviceReachableChanged,
                          _: &Connection,
                          _: &Message| {
                        let mut reachable = reachable_copy1.lock().unwrap();
                        *reachable = s.reachable;

                        send6
                            .send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap();

                        true
                    },
                );

                if old_kdeconnect {
                    let _battery_state_handler = p1.match_signal(
                        move |s: OrgKdeKdeconnectDeviceBatteryStateChanged,
                              _: &Connection,
                              _: &Message| {
                            let mut charging = charging_copy.lock().unwrap();
                            *charging = s.charging;

                            // Tell block to update now.
                            // The older KDEConnect emits both stateChanged and chargeChanged
                            // whenever there is an update regardless of whether or
                            // not they both changed. So we only need to send updates
                            // in one of the two battery signal handlers.
                            send.send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap();

                            true
                        },
                    );

                    let _battery_charge_handler = p1.match_signal(
                        move |s: OrgKdeKdeconnectDeviceBatteryChargeChanged,
                              _: &Connection,
                              _: &Message| {
                            let mut charge = charge_copy.lock().unwrap();
                            *charge = s.charge;

                            true
                        },
                    );
                } else {
                    let p2 = c.with_proxy(
                        "org.kde.kdeconnect",
                        format!("/modules/kdeconnect/devices/{}/battery", device_id_copy),
                        Duration::from_millis(5000),
                    );
                    let _battery_state_handler = p2.match_signal(
                        move |s: OrgKdeKdeconnectDeviceBatteryRefreshed,
                              _: &Connection,
                              _: &Message| {
                            let mut charging = charging_copy.lock().unwrap();
                            *charging = s.is_charging;

                            let mut charge = charge_copy.lock().unwrap();
                            *charge = s.charge;

                            send.send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap();

                            true
                        },
                    );
                };

                if old_kdeconnect {
                    let _notification_added_handler = p1.match_signal(
                        move |_s: OrgKdeKdeconnectDeviceNotificationsNotificationPosted,
                              _: &Connection,
                              _: &Message| {
                            let mut notif_count = notif_count_copy1.lock().unwrap();
                            *notif_count += 1;

                            // Tell block to update now.
                            send3
                                .send(Task {
                                    id,
                                    update_time: Instant::now(),
                                })
                                .unwrap();

                            true
                        },
                    );

                    let _notification_removed_handler = p1.match_signal(
                        move |_s: OrgKdeKdeconnectDeviceNotificationsNotificationRemoved,
                              _: &Connection,
                              _: &Message| {
                            let mut notif_count = notif_count_copy2.lock().unwrap();
                            *notif_count = if *notif_count - 1 < 0 {
                                0
                            } else {
                                *notif_count - 1
                            };

                            // Tell block to update now.
                            send4
                                .send(Task {
                                    id,
                                    update_time: Instant::now(),
                                })
                                .unwrap();

                            true
                        },
                    );

                    let _notification_all_removed_handler = p1.match_signal(
                        move |_s: OrgKdeKdeconnectDeviceNotificationsAllNotificationsRemoved,
                              _: &Connection,
                              _: &Message| {
                            let mut notif_count = notif_count_copy3.lock().unwrap();
                            *notif_count = 0;

                            // Tell block to update now.
                            send5
                                .send(Task {
                                    id,
                                    update_time: Instant::now(),
                                })
                                .unwrap();

                            true
                        },
                    );
                } else {
                    let p3 = c.with_proxy(
                        "org.kde.kdeconnect",
                        format!(
                            "/modules/kdeconnect/devices/{}/notifications",
                            device_id_copy
                        ),
                        Duration::from_millis(5000),
                    );

                    let _notification_added_handler = p3.match_signal(
                        move |_s: OrgKdeKdeconnectDeviceNotificationsNotificationPosted,
                              _: &Connection,
                              _: &Message| {
                            let mut notif_count = notif_count_copy1.lock().unwrap();
                            *notif_count += 1;

                            // Tell block to update now.
                            send3
                                .send(Task {
                                    id,
                                    update_time: Instant::now(),
                                })
                                .unwrap();

                            true
                        },
                    );

                    let _notification_removed_handler = p3.match_signal(
                        move |_s: OrgKdeKdeconnectDeviceNotificationsNotificationRemoved,
                              _: &Connection,
                              _: &Message| {
                            let mut notif_count = notif_count_copy2.lock().unwrap();
                            *notif_count = if *notif_count - 1 < 0 {
                                0
                            } else {
                                *notif_count - 1
                            };

                            // Tell block to update now.
                            send4
                                .send(Task {
                                    id,
                                    update_time: Instant::now(),
                                })
                                .unwrap();

                            true
                        },
                    );

                    let _notification_all_removed_handler = p3.match_signal(
                        move |_s: OrgKdeKdeconnectDeviceNotificationsAllNotificationsRemoved,
                              _: &Connection,
                              _: &Message| {
                            let mut notif_count = notif_count_copy3.lock().unwrap();
                            *notif_count = 0;

                            // Tell block to update now.
                            send5
                                .send(Task {
                                    id,
                                    update_time: Instant::now(),
                                })
                                .unwrap();

                            true
                        },
                    );

                    //if notif_text is ever implemented this may be handy
                    //OrgKdeKdeconnectDeviceNotificationsNotificationUpdated
                };

                let p4 = c.with_proxy(
                    "org.kde.kdeconnect",
                    "/modules/kdeconnect",
                    Duration::from_millis(5000),
                );

                let _phone_visible_handler = p4.match_signal(
                    move |s: OrgKdeKdeconnectDaemonDeviceVisibilityChanged,
                          _: &Connection,
                          _: &Message| {
                        // TODO: check if s.id matches our device? Is visible same as reachable?
                        let mut reachable = reachable_copy2.lock().unwrap();
                        *reachable = s.is_visible;

                        // Tell block to update now.
                        send7
                            .send(Task {
                                id,
                                update_time: Instant::now(),
                            })
                            .unwrap();

                        true
                    },
                );

                loop {
                    c.process(Duration::from_millis(1000)).unwrap();
                }
            })
            .unwrap();

        Ok(KDEConnect {
            id,
            device_id,
            device_name,
            battery_charge: charge,
            battery_state: charging,
            notif_count,
            // TODO
            //notif_text,
            phone_reachable: reachable,
            bat_good: block_config.bat_good,
            bat_info: block_config.bat_info,
            bat_warning: block_config.bat_warning,
            bat_critical: block_config.bat_critical,
            format: block_config
                .format
                .with_default("{name} {bat_icon}{bat_charge} {notif_icon}{notif_count}")?,
            format_disconnected: block_config.format_disconnected.with_default("{name}")?,
            output: TextWidget::new(id, 0, shared_config.clone()).with_icon("phone")?,
            shared_config,
        })
    }
}

impl Block for KDEConnect {
    fn id(&self) -> usize {
        self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let charge = (*self
            .battery_charge
            .lock()
            .block_error("kdeconnect", "failed to acquire lock for `charge`")?)
            as i32;

        let charging = *self
            .battery_state
            .lock()
            .block_error("kdeconnect", "failed to acquire lock for `battery_state`")?;

        let notif_count = *self
            .notif_count
            .lock()
            .block_error("kdeconnect", "failed to acquire lock for `notif_count`")?;

        // TODO
        //let notif_text = (*self
        //   .notif_text
        //   .lock()
        //   .block_error("kdeconnect", "failed to acquire lock for `notif_text`")?)
        //   .clone();

        let phone_reachable = *self
            .phone_reachable
            .lock()
            .block_error("kdeconnect", "failed to acquire lock for `phone_reachable`")?;

        let name = (*self
            .device_name
            .lock()
            .block_error("kdeconnect", "failed to acquire lock for `name`")?)
        .clone();

        let bat_icon = self
            .shared_config
            .get_icon(if charging {
                // TODO: there are icons for different states of charging
                "bat_charging"
            } else if charge < 0 {
                // better than nothing I guess?
                "bat_full"
            } else {
                battery_level_to_icon(
                    Ok(charge as u64),
                    match self.shared_config.get_icon("bat_10") {
                        Ok(_) => false,
                        Err(_) => {
                            eprintln!(
                                "Icon bat_10 not found in your icons file. Please check NEWS.md"
                            );
                            true
                        }
                    },
                )
            })
            .unwrap_or_default();

        let values = map!(
            "bat_icon" => Value::from_string(bat_icon.trim().to_string()),
            "bat_charge" => Value::from_integer(charge.clamp(0,100) as i64).percents(),
            "bat_state" => Value::from_string(charging.to_string()),
            "notif_icon" => Value::from_string(self.shared_config.get_icon("notification").unwrap_or_default().trim().to_string()),
            "notif_count" => Value::from_integer(notif_count as i64),
            // TODO
            //"notif_text" => notif_text,
            "name" => Value::from_string(name),
            "id" => Value::from_string(self.device_id.to_string()) // Not a String?
        );

        if (
            self.bat_critical,
            self.bat_warning,
            self.bat_info,
            self.bat_good,
        ) == (0, 0, 0, 0)
        {
            self.output.set_state(match notif_count {
                0 => State::Idle,
                _ => State::Info,
            })
        } else if charging {
            self.output.set_state(State::Good);
        } else {
            self.output.set_state(if charge <= self.bat_critical {
                State::Critical
            } else if charge <= self.bat_warning {
                State::Warning
            } else if charge <= self.bat_info {
                State::Info
            } else if charge > self.bat_good {
                State::Good
            } else {
                State::Idle
            });
        }

        if !phone_reachable {
            self.output.set_state(State::Critical);
            self.output.set_icon("phone_disconnected")?;
            self.output
                .set_texts(self.format_disconnected.render(&values)?);
        } else {
            self.output.set_icon("phone")?;
            self.output.set_texts(self.format.render(&values)?);
        }

        Ok(None)
    }

    // Returns the view of the block, comprised of widgets.
    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }
}

// Code below generated using the command below and Results changed to explcitly use std::Result
// `dbus-codegen-rust -d org.kde.kdeconnect -p /modules/kdeconnect/devices/mydeviceid/battery`
#[derive(Debug)]
pub struct OrgKdeKdeconnectDeviceBatteryRefreshed {
    pub is_charging: bool,
    pub charge: i32,
}

impl arg::AppendAll for OrgKdeKdeconnectDeviceBatteryRefreshed {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.is_charging, i);
        arg::RefArg::append(&self.charge, i);
    }
}

impl arg::ReadAll for OrgKdeKdeconnectDeviceBatteryRefreshed {
    fn read(i: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDeviceBatteryRefreshed {
            is_charging: i.read()?,
            charge: i.read()?,
        })
    }
}

// Code below generated using the command below and Results changed to explcitly use std::Result
// `dbus-codegen-rust -d org.kde.kdeconnect -p /modules/kdeconnect/devices/mydeviceid/notifications`
#[derive(Debug)]
pub struct OrgKdeKdeconnectDeviceNotificationsNotificationPosted {
    pub public_id: String,
}

impl arg::AppendAll for OrgKdeKdeconnectDeviceNotificationsNotificationPosted {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.public_id, i);
    }
}

impl arg::ReadAll for OrgKdeKdeconnectDeviceNotificationsNotificationPosted {
    fn read(i: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDeviceNotificationsNotificationPosted {
            public_id: i.read()?,
        })
    }
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDeviceNotificationsNotificationPosted {
    const NAME: &'static str = "notificationPosted";
    const INTERFACE: &'static str = "org.kde.kdeconnect.device.notifications";
}

#[derive(Debug)]
pub struct OrgKdeKdeconnectDeviceNotificationsNotificationRemoved {
    pub public_id: String,
}

impl arg::AppendAll for OrgKdeKdeconnectDeviceNotificationsNotificationRemoved {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.public_id, i);
    }
}

impl arg::ReadAll for OrgKdeKdeconnectDeviceNotificationsNotificationRemoved {
    fn read(i: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDeviceNotificationsNotificationRemoved {
            public_id: i.read()?,
        })
    }
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDeviceNotificationsNotificationRemoved {
    const NAME: &'static str = "notificationRemoved";
    const INTERFACE: &'static str = "org.kde.kdeconnect.device.notifications";
}

#[derive(Debug)]
pub struct OrgKdeKdeconnectDeviceNotificationsNotificationUpdated {
    pub public_id: String,
}

impl arg::AppendAll for OrgKdeKdeconnectDeviceNotificationsNotificationUpdated {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.public_id, i);
    }
}

impl arg::ReadAll for OrgKdeKdeconnectDeviceNotificationsNotificationUpdated {
    fn read(i: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDeviceNotificationsNotificationUpdated {
            public_id: i.read()?,
        })
    }
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDeviceNotificationsNotificationUpdated {
    const NAME: &'static str = "notificationUpdated";
    const INTERFACE: &'static str = "org.kde.kdeconnect.device.notifications";
}

#[derive(Debug)]
pub struct OrgKdeKdeconnectDeviceNotificationsAllNotificationsRemoved {}

impl arg::AppendAll for OrgKdeKdeconnectDeviceNotificationsAllNotificationsRemoved {
    fn append(&self, _: &mut arg::IterAppend) {}
}

impl arg::ReadAll for OrgKdeKdeconnectDeviceNotificationsAllNotificationsRemoved {
    fn read(_: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDeviceNotificationsAllNotificationsRemoved {})
    }
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDeviceNotificationsAllNotificationsRemoved {
    const NAME: &'static str = "allNotificationsRemoved";
    const INTERFACE: &'static str = "org.kde.kdeconnect.device.notifications";
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDeviceBatteryRefreshed {
    const NAME: &'static str = "refreshed";
    const INTERFACE: &'static str = "org.kde.kdeconnect.device.battery";
}

// Code below generated using the command below and Results changed to explcitly use std::Result
// `dbus-codegen-rust -d org.kde.kdeconnect -p /modules/kdeconnect/devices/mydeviceid`
#[derive(Debug)]
pub struct OrgKdeKdeconnectDeviceNameChanged {
    pub name: String,
}

impl arg::AppendAll for OrgKdeKdeconnectDeviceNameChanged {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.name, i);
    }
}

impl arg::ReadAll for OrgKdeKdeconnectDeviceNameChanged {
    fn read(i: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDeviceNameChanged { name: i.read()? })
    }
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDeviceNameChanged {
    const NAME: &'static str = "nameChanged";
    const INTERFACE: &'static str = "org.kde.kdeconnect.device";
}

#[derive(Debug)]
pub struct OrgKdeKdeconnectDeviceReachableChanged {
    pub reachable: bool,
}

impl arg::AppendAll for OrgKdeKdeconnectDeviceReachableChanged {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.reachable, i);
    }
}

impl arg::ReadAll for OrgKdeKdeconnectDeviceReachableChanged {
    fn read(i: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDeviceReachableChanged {
            reachable: i.read()?,
        })
    }
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDeviceReachableChanged {
    const NAME: &'static str = "reachableChanged";
    const INTERFACE: &'static str = "org.kde.kdeconnect.device";
}

// This code was autogenerated using the command below and Results changed to explcitly use std::Result
// `dbus-codegen-rust -d org.kde.kdeconnect -p /modules/kdeconnect`
#[derive(Debug)]
pub struct OrgKdeKdeconnectDaemonDeviceAdded {
    pub id: String,
}

impl arg::AppendAll for OrgKdeKdeconnectDaemonDeviceAdded {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.id, i);
    }
}

impl arg::ReadAll for OrgKdeKdeconnectDaemonDeviceAdded {
    fn read(i: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDaemonDeviceAdded { id: i.read()? })
    }
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDaemonDeviceAdded {
    const NAME: &'static str = "deviceAdded";
    const INTERFACE: &'static str = "org.kde.kdeconnect.daemon";
}

#[derive(Debug)]
pub struct OrgKdeKdeconnectDaemonDeviceRemoved {
    pub id: String,
}

impl arg::AppendAll for OrgKdeKdeconnectDaemonDeviceRemoved {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.id, i);
    }
}

impl arg::ReadAll for OrgKdeKdeconnectDaemonDeviceRemoved {
    fn read(i: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDaemonDeviceRemoved { id: i.read()? })
    }
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDaemonDeviceRemoved {
    const NAME: &'static str = "deviceRemoved";
    const INTERFACE: &'static str = "org.kde.kdeconnect.daemon";
}

#[derive(Debug)]
pub struct OrgKdeKdeconnectDaemonDeviceVisibilityChanged {
    pub id: String,
    pub is_visible: bool,
}

impl arg::AppendAll for OrgKdeKdeconnectDaemonDeviceVisibilityChanged {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.id, i);
        arg::RefArg::append(&self.is_visible, i);
    }
}

impl arg::ReadAll for OrgKdeKdeconnectDaemonDeviceVisibilityChanged {
    fn read(i: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDaemonDeviceVisibilityChanged {
            id: i.read()?,
            is_visible: i.read()?,
        })
    }
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDaemonDeviceVisibilityChanged {
    const NAME: &'static str = "deviceVisibilityChanged";
    const INTERFACE: &'static str = "org.kde.kdeconnect.daemon";
}

#[derive(Debug)]
pub struct OrgKdeKdeconnectDaemonDeviceListChanged {}

impl arg::AppendAll for OrgKdeKdeconnectDaemonDeviceListChanged {
    fn append(&self, _: &mut arg::IterAppend) {}
}

impl arg::ReadAll for OrgKdeKdeconnectDaemonDeviceListChanged {
    fn read(_: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDaemonDeviceListChanged {})
    }
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDaemonDeviceListChanged {
    const NAME: &'static str = "deviceListChanged";
    const INTERFACE: &'static str = "org.kde.kdeconnect.daemon";
}

// these are for kdeconnect versions 20.08.3 and lower
#[derive(Debug)]
pub struct OrgKdeKdeconnectDeviceBatteryStateChanged {
    pub charging: bool,
}

impl arg::AppendAll for OrgKdeKdeconnectDeviceBatteryStateChanged {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.charging, i);
    }
}

impl arg::ReadAll for OrgKdeKdeconnectDeviceBatteryStateChanged {
    fn read(i: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDeviceBatteryStateChanged {
            charging: i.read()?,
        })
    }
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDeviceBatteryStateChanged {
    const NAME: &'static str = "stateChanged";
    const INTERFACE: &'static str = "org.kde.kdeconnect.device.battery";
}

#[derive(Debug)]
pub struct OrgKdeKdeconnectDeviceBatteryChargeChanged {
    pub charge: i32,
}

impl arg::AppendAll for OrgKdeKdeconnectDeviceBatteryChargeChanged {
    fn append(&self, i: &mut arg::IterAppend) {
        arg::RefArg::append(&self.charge, i);
    }
}

impl arg::ReadAll for OrgKdeKdeconnectDeviceBatteryChargeChanged {
    fn read(i: &mut arg::Iter) -> std::result::Result<Self, arg::TypeMismatchError> {
        Ok(OrgKdeKdeconnectDeviceBatteryChargeChanged { charge: i.read()? })
    }
}

impl dbus::message::SignalArgs for OrgKdeKdeconnectDeviceBatteryChargeChanged {
    const NAME: &'static str = "chargeChanged";
    const INTERFACE: &'static str = "org.kde.kdeconnect.device.battery";
}
