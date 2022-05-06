use virt::connect::Connect;

use std::time::Duration;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;
use crate::widgets::State;

pub struct Libvirt {
    id: usize,
    text: TextWidget,
    format: FormatTemplate,
    update_interval: Duration,
    qemu_conn: Connect,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct LibvirtConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    /// Format override
    pub format: FormatTemplate,

    /// URL to QEMU
    pub qemu_url: String,
}

impl Default for LibvirtConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            format: FormatTemplate::default(),
            qemu_url: "qemu:///system".to_string(),
        }
    }
}

impl ConfigBlock for Libvirt {
    type Config = LibvirtConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _: Sender<Task>,
    ) -> Result<Self> {
        let text = TextWidget::new(id, 0, shared_config)
            .with_text("vms")
            .with_icon("virtual-machine")?;
        Ok(Libvirt {
            id,
            text,
            format: block_config.format.with_default("{running}")?,
            update_interval: block_config.interval,
            qemu_conn: Connect::open_read_only(&block_config.qemu_url)
                .expect("could not connect to libvirtd"),
        })
    }
}

impl Block for Libvirt {
    fn update(&mut self) -> Result<Option<Update>> {
        if !self
            .qemu_conn
            .is_alive()
            .expect("unrecoverable error with qemu object")
        {
            self.qemu_conn = Connect::open_read_only(
                &self
                    .qemu_conn
                    .get_uri()
                    .expect("could not get URI from the currently connected QEMU object"),
            )
            .expect("could not re-connect to libvirtd")
        };

        let mut paused: i64 = 0;
        match self.qemu_conn.list_all_domains(1 << 5) {
            Ok(d) => paused = d.len() as i64,
            Err(e) => eprintln!("{}", e),
        }

        let mut stopped: i64 = 0;
        match self.qemu_conn.num_of_defined_domains() {
            Ok(d) => stopped = d as i64,
            Err(e) => eprintln!("{}", e),
        };

        let mut running = 0;
        match self.qemu_conn.list_all_domains(1 << 4) {
            Ok(d) => running = d.len() as i64,
            Err(e) => eprintln!("{}", e),
        };

        let total = running + stopped + paused;

        let mut num_images: i64 = 0;
        match self.qemu_conn.list_all_storage_pools(1 << 1) {
            Ok(pools) => {
                for pool in pools {
                    num_images += pool
                        .num_of_volumes()
                        .expect("could not get number of volumes in pool")
                        as i64;
                }
            }
            Err(e) => eprintln!("{}", e),
        };

        let values = map!(
            "total" =>   Value::from_integer(total),
            "running" => Value::from_integer(running),
            "paused" =>  Value::from_integer(paused),
            "stopped" => Value::from_integer(stopped),
            "images" =>  Value::from_integer(num_images),
        );

        self.text.set_texts(self.format.render(&values)?);
        self.text.set_state(State::Idle);

        Ok(Some(self.update_interval.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn id(&self) -> usize {
        self.id
    }
}
