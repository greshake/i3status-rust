use std::thread;
use std::time::Instant;

use crossbeam_channel::Sender;
use dbus::ffidisp::{BusType, Connection, ConnectionItem};
use serde::{Deserialize as des, Serialize as ser};
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::http;
use crate::scheduler::Task;
use crate::util::country_flag_from_iso_code;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};
use crate::Duration;

const API_ENDPOINT: &str = "http://ip-api.com/json";

#[derive(ser, des, Default)]
struct IPAddressInfo {
    #[serde(rename = "query")]
    address: String,
    status: String,
    country: String,
    #[serde(rename = "countryCode")]
    country_code: String,
    region: String,
    #[serde(rename = "regionName")]
    region_name: String,
    city: String,
    zip: String,
    lat: f64,
    lon: f64,
    timezone: String,
    isp: String,
    org: String,
    #[serde(rename = "as")]
    autonomous_system: String,
}

pub struct ExternalIP {
    id: usize,
    output: TextWidget,
    format: FormatTemplate,
    refresh_interval_success: u64,
    refresh_interval_failure: u64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct ExternalIPConfig {
    /// External IP formatter.
    pub format: FormatTemplate,
    pub refresh_interval_success: u64,
    pub refresh_interval_failure: u64,
}

impl Default for ExternalIPConfig {
    fn default() -> Self {
        Self {
            format: FormatTemplate::default(),
            refresh_interval_success: 300,
            refresh_interval_failure: 15,
        }
    }
}

impl ConfigBlock for ExternalIP {
    type Config = ExternalIPConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        send: Sender<Task>,
    ) -> Result<Self> {
        thread::Builder::new()
            .name("externalip".into())
            .spawn(move || {
                let c = Connection::get_private(BusType::System).unwrap();
                c.add_match(
                    "type='signal',\
                    path='/org/freedesktop/NetworkManager',\
                    interface='org.freedesktop.DBus.Properties',\
                    member='PropertiesChanged'",
                )
                .unwrap();
                c.add_match(
                    "type='signal',\
                    path_namespace='/org/freedesktop/NetworkManager/ActiveConnection',\
                    interface='org.freedesktop.DBus.Properties',\
                    member='PropertiesChanged'",
                )
                .unwrap();
                c.add_match(
                    "type='signal',\
                    path_namespace='/org/freedesktop/NetworkManager/IP4Config',\
                    interface='org.freedesktop.DBus',\
                    member='PropertiesChanged'",
                )
                .unwrap();

                loop {
                    let timeout = 300_000;

                    for event in c.iter(timeout) {
                        match event {
                            ConnectionItem::Nothing => (),
                            _ => {
                                send.send(Task {
                                    id,
                                    update_time: Instant::now(),
                                })
                                .unwrap();
                            }
                        }
                    }
                }
            })
            .unwrap();

        Ok(ExternalIP {
            id,
            output: TextWidget::new(id, 0, shared_config),
            format: block_config
                .format
                .with_default("{address} {country_flag}")?,
            refresh_interval_success: block_config.refresh_interval_success,
            refresh_interval_failure: block_config.refresh_interval_failure,
        })
    }
}

impl Block for ExternalIP {
    fn id(&self) -> usize {
        self.id
    }

    fn update(&mut self) -> Result<Option<Update>> {
        let (external_ip, success) = {
            let ip_info =
                match http::http_get_json(API_ENDPOINT, Some(Duration::from_secs(3)), vec![]) {
                    Ok(ip_info_json) => serde_json::from_value(ip_info_json.content).unwrap(),
                    _ => IPAddressInfo::default(),
                };
            match ip_info.status.as_ref() {
                "success" => {
                    self.output.set_state(State::Idle);
                    let flag = country_flag_from_iso_code(ip_info.country_code.as_str());
                    let values = map!(
                        "address" => Value::from_string (ip_info.address),
                        "country" => Value::from_string (ip_info.country),
                        "country_code" => Value::from_string (ip_info.country_code),
                        "region" => Value::from_string (ip_info.region),
                        "region_name" => Value::from_string (ip_info.region_name),
                        "city" => Value::from_string (ip_info.city),
                        "zip" => Value::from_string (ip_info.zip),
                        "latitude" => Value::from_float (ip_info.lat),
                        "longitude" => Value::from_float (ip_info.lon),
                        "timezone" => Value::from_string (ip_info.timezone),
                        "isp" => Value::from_string (ip_info.isp),
                        "org" => Value::from_string (ip_info.org),
                        "autonomous_system" => Value::from_string (ip_info.autonomous_system),
                        "country_flag" => Value::from_string(flag),
                    );
                    let s = self.format.render(&values)?;
                    (s.0, true)
                }
                _ => {
                    self.output.set_state(State::Critical);
                    ("Request to IP service failed".to_string(), false)
                }
            }
        };
        self.output.set_text(external_ip);
        match success {
            /* The external IP address can change without triggering a
             * notification (for example a refresh between the router and
             * the ISP) so check from time to time even on success */
            true => Ok(Some(
                Duration::from_secs(self.refresh_interval_success).into(),
            )),
            false => Ok(Some(
                Duration::from_secs(self.refresh_interval_failure).into(),
            )),
        }
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.output]
    }
}
