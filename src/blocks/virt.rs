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
//! `active` | Virtual machines running on the host   | Number | -
//! `inactive` | Virtual machines stopped on the host   | Number | -
//! `total`  | Virtual machines in total the host    | Number | -
//! `memory_active   | Total memory used by running virtual machines | Number | -
//! `memory_max`     | Total memory used by virtual machines of any state | Number | -
//! `cpu_active`     | Total cpu cores used by the virtual machines | Number | -
//! `cpu_inactive`   | Total cpu used by stopped virtual machines | Number | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "virt"
//! uri = "qemu:///system"
//! interval = 2
//! format = " $icon $active/$total ($memory_activey@$cpu_active) "
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

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default("$icon $active/$total")?;
    let mut widget = Widget::new().with_format(format.clone());

    let flags: sys::virConnectListAllDomainsFlags = sys::VIR_CONNECT_LIST_DOMAINS_ACTIVE | sys::VIR_CONNECT_LIST_DOMAINS_INACTIVE;
    let uri: &str = "qemu:///system";

    dbg!(&widget);

    loop {
        println!("Connecting to hypervisor '{}'", &uri);
        let mut con = match Connect::open(uri) {
            Ok(c) => c,
            Err(e) => panic!("No connection to hypervisor: {}", e),
        };

        let info: LibvirtInfo = LibvirtInfo::new(&mut con, flags).unwrap();

        widget.set_values(map!(
            "icon" => Value::icon("".to_string()),
            // "total" => Value::number(virt_active_doms + virt_inactive_doms),
            "running" => Value::number(info.active),
            "stopped" => Value::number(info.inactive),
            // "paused" => Value::number(virt_inactive_domains),
            "total" => Value::number(info.total),
        ));

        api.set_widget(widget.clone())?;

        println!("Disconnecting from hypervisor");
        disconnect(&mut con);
        
        select! {
            _ = sleep(config.interval.0) => (),
        }
        
    }
}


#[derive(Deserialize, Debug)]
struct LibvirtInfo {
    #[serde(rename = "VMActive")]
    active:  u32,
    #[serde(rename = "VMInactive")]
    inactive: u32,
    #[serde(rename = "VMTotal")]
    total: u32,
    #[serde(rename = "MemoryActive")]
    memory_active: u32,
    #[serde(rename = "MemoryInactive")]
    memory_max: u32,
    #[serde(rename = "CPUActive")]
    cpu_active: u32,
    #[serde(rename = "CPUInactive")]
    cpu_inactive: u32,
}

fn disconnect(con: &mut Connect) {
    if let Err(e) = con.close() {
        panic!("Failed to disconnect from hypervisor: {}", e);
    }
    println!("Disconnected from hypervisor");
}

impl LibvirtInfo {
    pub fn new(con: &mut Connect, flags: sys::virConnectListAllDomainsFlags) -> Result<Self, Error> {
        println!("Connected to hypervisor");
        match con.get_uri() {
            Ok(u) => println!("Connected to hypervisor at '{}'", u),
            Err(e) => {
                disconnect(con);
                panic!("Failed to get URI for hypervisor connection: {}", e);
            }
        };

        println!("Getting information about domains");
        if let Ok(virt_active_domains) = con.num_of_domains() {
            if let Ok(virt_inactive_domains) = con.num_of_defined_domains() {
                if let Ok(domains) = con.list_all_domains(flags) {
                    let mut info = LibvirtInfo {
                        active: virt_active_domains,
                        inactive: virt_inactive_domains,
                        total: virt_active_domains + virt_inactive_domains,
                        memory_active: 0,
                        memory_max: 0,
                        cpu_active: 0,
                        cpu_inactive: 0,
                    };

                    for domain in domains {
                        if let Ok(domain_info) = domain.get_info() {
                            info.memory_max += domain_info.max_mem as u32;

                            if domain.is_active().unwrap_or(false) {
                                info.memory_active += domain_info.memory as u32;
                                info.cpu_active += domain_info.nr_virt_cpu as u32;
                            } else {
                                info.cpu_inactive += domain_info.nr_virt_cpu as u32;
                            }
                        }
                    }

                    return Ok(info);
                }
                else {
                    disconnect(con);
                    return Err(Error::last_error());
                }
                
            }
            else {
                disconnect(con);
                return Err(Error::last_error());
            }
        }
        else {
            disconnect(con);
            return Err(Error::last_error());
        }
    }
}

