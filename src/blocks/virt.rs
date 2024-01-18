//! Local virtual machine state.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! uri | URI of the hypervisor | qemu:///system
//! `interval` | Update interval, in seconds. | `5`
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $running.eng(w:1) "`
//! `uru` | The path to the virtualization domain.
//!
//! Key       | Value                                  | Type   | Unit
//! ----------|----------------------------------------|--------|-----
//! `icon`    | A static icon                          | Icon   | -
//! `total`   | Total containers on the host           | Number | -
//! `running` | Virtual machines running on the host   | Number | -
//! `stopped` | Virtual machines stopped on the host   | Number | -
//! `paused`  | Virtual machines paused on the host    | Number | -
//! `total`   | Total Virtual machines on the host     | Number | -
//! `memory   | Total memory used by the virtual machines | Number |
//! `cpu`     | Total percentage cpu used by the virtual machines | Number |
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "virt"
//! uri = "qemu:///system"
//! interval = 2
//! format = " $icon $running/$total ($memory@$cpu) "
//! ```
//!

use super::prelude::*;
use virt::connect::Connect;
use virt::error::Error;
use virt::sys;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    #[default("qemu:///system".into())]
    pub uri: ShellString,
    pub format: FormatConfig,
    #[default(5.into())]
    pub interval: Seconds,
}

fn disconnect(mut conn: Connect) {
    if let Err(e) = conn.close() {
        panic!("Failed to disconnect from hypervisor: {}", e);
    }
    println!("Disconnected from hypervisor");
}


pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default("$icon $running/$total")?;
    let mut widget = Widget::new().with_format(format.clone());
    let flags = sys::VIR_CONNECT_LIST_DOMAINS_ACTIVE | sys::VIR_CONNECT_LIST_DOMAINS_INACTIVE;
    let uri: &str = "qemu:///system";

    let mut timer = config.interval.timer();
    dbg!(&format);

    loop {
        // 0. connect to hypervisor 
        println!("Connecting to hypervisor");
        let con = match Connect::open(uri) {
            Ok(c) => c,
            Err(e) => panic!("No connection to hypervisor: {}", e),
        };

        println!("Connected to hypervisor");
        match con.get_uri() {
            Ok(u) => println!("Connected to hypervisor at '{}'", u),
            Err(e) => {
                disconnect(con);
                panic!("Failed to get URI for hypervisor connection: {}", e);
            }
        };

        // 1. get all the information
        println!("Getting information about domains");
        if let Ok(virt_active_domains) = con.num_of_domains() {
            if let Ok(virt_inactive_domains) = con.num_of_defined_domains() {
                println!(
                    "There are {} active and {} inactive domains",
                    virt_active_domains, virt_inactive_domains
                );
                
                widget.set_values(map!(
                    "icon" => Value::icon("".to_string()),
                    // "total" => Value::number(virt_active_doms + virt_inactive_doms),
                    "running" => Value::number(virt_active_domains),
                    // "stopped" => Value::number(virt_inactive_doms),
                    // "paused" => Value::number(virt_inactive_domains),
                    "total" => Value::number(virt_active_domains + virt_inactive_domains),
                ));

                dbg!(&widget);
                api.set_widget(widget.clone())?;
            }
        }
        
        select! {
            _ = sleep(config.interval.0) => (),
        }
        
        // 2. disconnect
        disconnect(con);
    }
}


#[derive(Deserialize, Debug)]
struct LibvirtInfo {
    #[serde(rename = "VMTotal")]
    total: i64,
    #[serde(rename = "VMActive")]
    active: i64,
    #[serde(rename = "VMInactive")]
    inactive: i64,
}

impl LibvirtInfo {
    // Create a new `LibvirtInfo` struct.
    fn new(con: &Connect) -> Self {
        LibvirtInfo {
            total: 0,
            active: 0,
            inactive: 0,
        }
    }
}
