use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use dbus::arg::RefArg;
use dbus::ffidisp::stdintf::OrgFreedesktopDBusProperties;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::config::{LogicalDirection, Scrolling};
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::protocol::i3bar_event::{I3BarEvent, MouseButton};
use crate::scheduler::Task;
use crate::subprocess::spawn_child_async;
use crate::util::has_command;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

pub struct Hueshift {
    id: usize,
    text: TextWidget,
    // update_interval: Duration,
    step: u16,
    current_temp: u16,
    max_temp: u16,
    min_temp: u16,
    hue_shifter: HueShifter,
    hue_shift_driver: Box<dyn HueShiftDriver>,
    click_temp: u16,
    scrolling: Scrolling,
}

trait HueShiftDriver {
    fn update(&self, temp: u16) -> Result<()>;
    fn reset(&self) -> Result<()>;
    fn get_current_temperature(&mut self) -> Result<Option<u16>> {
        Ok(None)
    }
}
struct Redshift();
impl HueShiftDriver for Redshift {
    fn update(&self, temp: u16) -> Result<()> {
        spawn_child_async(
            "sh",
            &[
                "-c",
                format!("redshift -O {} -P >/dev/null 2>&1", temp).as_str(),
            ],
        )
        .block_error(
            "hueshift",
            "Failed to set new color temperature using redshift.",
        )?;
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        spawn_child_async("sh", &["-c", "redshift -x >/dev/null 2>&1"]).block_error(
            "redshift",
            "Failed to set new color temperature using redshift.",
        )?;
        Ok(())
    }
}
struct Sct();
impl HueShiftDriver for Sct {
    fn update(&self, temp: u16) -> Result<()> {
        spawn_child_async(
            "sh",
            &["-c", format!("sct {} >/dev/null 2>&1", temp).as_str()],
        )
        .block_error("hueshift", "Failed to set new color temperature using sct.")?;
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        spawn_child_async("sh", &["-c", "sct >/dev/null 2>&1"])
            .block_error("hueshift", "Failed to set new color temperature using sct.")?;
        Ok(())
    }
}
struct Gammastep();
impl HueShiftDriver for Gammastep {
    fn update(&self, temp: u16) -> Result<()> {
        spawn_child_async(
            "sh",
            &[
                "-c",
                &format!("pkill gammastep; gammastep -O {} -P &", temp),
            ],
        )
        .block_error(
            "hueshift",
            "Failed to set new color temperature using gammastep.",
        )?;
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        spawn_child_async("sh", &["-c", "gammastep -x >/dev/null 2>&1"]).block_error(
            "hueshift",
            "Failed to set new color temperature using gammastep.",
        )?;
        Ok(())
    }
}
struct Wlsunset();
impl HueShiftDriver for Wlsunset {
    fn update(&self, temp: u16) -> Result<()> {
        // wlsunset does not have a oneshot option, so set both day and
        // night temperature. wlsunset dose not allow for day and night
        // temperatures to be the same, so increment the day temperature.
        spawn_child_async(
            "sh",
            &[
                "-c",
                &format!("pkill wlsunset; wlsunset -T {} -t {} &", temp + 1, temp),
            ],
        )
        .block_error(
            "hueshift",
            "Failed to set new color temperature using wlsunset.",
        )?;
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        // wlsunset does not have a reset option, so just kill the process.
        // Trying to call wlsunset without any arguments uses the defaults:
        // day temp: 6500K
        // night temp: 4000K
        // latitude/longitude: NaN
        //     ^ results in sun_condition == POLAR_NIGHT at time of testing
        // With these defaults, this results in the the color temperature
        // getting set to 4000K.
        spawn_child_async("sh", &["-c", "pkill wlsunset > /dev/null 2>&1"]).block_error(
            "hueshift",
            "Failed to set new color temperature using wlsunset.",
        )?;
        Ok(())
    }
}

struct WlGammarelay {
    con: dbus::ffidisp::Connection,
    current_temperature: Arc<AtomicU16>,
}

impl WlGammarelay {
    fn attempt_to_get_current_temperature(
        con: &dbus::ffidisp::Connection,
        delay: u64,
        max_attempts: usize,
    ) -> Result<u16> {
        for attempt in 1..=max_attempts {
            match con
                .with_path("rs.wl-gammarelay", "/", 1000)
                .get::<u16>("rs.wl.gammarelay", "Temperature")
            {
                Ok(temperature) => {
                    return Ok(temperature);
                }
                Err(_) => {
                    if attempt == max_attempts {
                        return Err(BlockError(
                            "hueshift".to_string(),
                            "Unable to get current temperature for rs.wl.gammarelay".to_string(),
                        ));
                    } else {
                        thread::sleep(Duration::from_millis(delay));
                    }
                }
            }
        }
        Ok(0)
    }

    fn new(command: &str, id: usize, update_request: Sender<Task>) -> Result<Self> {
        spawn_child_async(
            "sh",
            &["-c", format!("{} >/dev/null 2>&1", command).as_str()],
        )
        .block_error("hueshift", format!("Failed to start {}.", command).as_str())?;
        let con = dbus::ffidisp::Connection::new_session()
            .block_error("hueshift", "Failed to establish D-Bus connection.")?;

        let current_temperature: Arc<AtomicU16> = Arc::new(AtomicU16::new(
            WlGammarelay::attempt_to_get_current_temperature(&con, 100, 5)?,
        ));

        {
            let current_temperature = current_temperature.clone();
            thread::Builder::new()
                .name("hueshift".into())
                .spawn(move || {
                    let con = dbus::ffidisp::Connection::new_session()
                        .expect("Failed to establish D-Bus connection.");

                    con.add_match(
                        "type='signal',\
                            interface='org.freedesktop.DBus.Properties',\
                            member='PropertiesChanged',\
                            arg0namespace='rs.wl.gammarelay'",
                    )
                    .expect("Failed to add D-Bus match rule.");

                    // First we're going to get an (irrelevant) NameAcquired event.
                    con.incoming(10_000).next();

                    loop {
                        if let Some(message) = con.incoming(10_000).next() {
                            if let (_, Some(changed_properties)) =
                                message.get2::<String, dbus::arg::PropMap>()
                            {
                                if let Some(temperature_variant) =
                                    changed_properties.get("Temperature")
                                {
                                    if let Some(temperature) = temperature_variant.as_u64() {
                                        let temperature = temperature as u16;
                                        current_temperature.store(temperature, Ordering::SeqCst);
                                        update_request
                                            .send(Task {
                                                id,
                                                update_time: Instant::now(),
                                            })
                                            .unwrap();
                                    }
                                }
                            }
                        }
                    }
                })
                .unwrap();
        }

        Ok(WlGammarelay {
            con,
            current_temperature,
        })
    }
}

impl HueShiftDriver for WlGammarelay {
    fn update(&self, temp: u16) -> Result<()> {
        self.con
            .with_path("rs.wl-gammarelay", "/", 1000)
            .set("rs.wl.gammarelay", "Temperature", temp)
            .map_err(|e| BlockError("hueshift".to_string(), e.to_string()))?;
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        // wl-gammarelay does not have a reset option just set the temp back to 6500
        self.update(6500)
    }

    fn get_current_temperature(&mut self) -> Result<Option<u16>> {
        Ok(Some(self.current_temperature.load(Ordering::SeqCst)))
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum HueShifter {
    Redshift,
    Sct,
    Gammastep,
    Wlsunset,
    WlGammarelay,
    WlGammarelayRs,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct HueshiftConfig {
    /// Update interval in seconds
    #[serde(deserialize_with = "deserialize_duration")]
    pub interval: Duration,

    pub max_temp: u16,
    pub min_temp: u16,

    // TODO: Detect currently defined temperature
    /// Currently defined temperature default to 6500K.
    pub current_temp: u16,

    /// Can be set by user as an option.
    pub hue_shifter: Option<HueShifter>,

    /// Default to 100K, cannot go over 500K.
    pub step: u16,
    pub click_temp: u16,
}

impl Default for HueshiftConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            max_temp: 10_000,
            min_temp: 1_000,
            current_temp: 6_500,
            hue_shifter: if has_command("hueshift", "redshift").unwrap_or(false) {
                Some(HueShifter::Redshift)
            } else if has_command("hueshift", "sct").unwrap_or(false) {
                Some(HueShifter::Sct)
            } else if has_command("hueshift", "gammastep").unwrap_or(false) {
                Some(HueShifter::Gammastep)
            } else if has_command("hueshift", "wlsunset").unwrap_or(false) {
                Some(HueShifter::Wlsunset)
            } else if has_command("hueshift", "wl-gammarelay-rs").unwrap_or(false) {
                Some(HueShifter::WlGammarelayRs)
            } else if has_command("hueshift", "wl-gammarelay").unwrap_or(false) {
                Some(HueShifter::WlGammarelay)
            } else {
                None
            },
            step: 100,
            click_temp: 6_500,
        }
    }
}

impl ConfigBlock for Hueshift {
    type Config = HueshiftConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        update_request: Sender<Task>,
    ) -> Result<Self> {
        let current_temp = block_config.current_temp;
        let mut step = block_config.step;
        let mut max_temp = block_config.max_temp;
        let mut min_temp = block_config.min_temp;
        // limit too big steps at 500K to avoid too brutal changes
        if step > 500 {
            step = 500;
        }
        if block_config.max_temp > 10_000 {
            max_temp = 10_000;
        }
        if block_config.min_temp < 1000 || block_config.min_temp > block_config.max_temp {
            min_temp = 1000;
        }

        let hue_shifter = block_config
            .hue_shifter
            .block_error("hueshift", "Cound not detect driver program")?;

        let hue_shift_driver: Box<dyn HueShiftDriver> = match hue_shifter {
            HueShifter::Redshift => Box::new(Redshift {}),
            HueShifter::Sct => Box::new(Sct {}),
            HueShifter::Gammastep => Box::new(Gammastep {}),
            HueShifter::Wlsunset => Box::new(Wlsunset {}),
            HueShifter::WlGammarelayRs => {
                Box::new(WlGammarelay::new("wl-gammarelay-rs", id, update_request)?)
            }
            HueShifter::WlGammarelay => {
                Box::new(WlGammarelay::new("wl-gammarelay", id, update_request)?)
            }
        };

        Ok(Hueshift {
            id,
            // update_interval: block_config.interval,
            step,
            max_temp,
            min_temp,
            current_temp,
            hue_shifter,
            hue_shift_driver,
            click_temp: block_config.click_temp,
            scrolling: shared_config.scrolling,
            text: TextWidget::new(id, 0, shared_config).with_text(&current_temp.to_string()),
        })
    }
}

impl Block for Hueshift {
    fn update(&mut self) -> Result<Option<Update>> {
        if let Some(current_temp) = self.hue_shift_driver.get_current_temperature()? {
            self.current_temp = current_temp;
        }
        self.text.set_text(self.current_temp.to_string());
        // If drivers have a way of polling for the current temperature then it
        // makes sense to have an update interval otherwise it has no effect.
        // None of the drivers besides WlGammarelay has a mechanism to get the
        // current temperature if they are changed outside of the statusbar.
        // Although WlGammarelay can get the current temperature it doesn't need
        // to run update on an update interval as it is listening to dbus events.
        // Something like this:
        Ok(match self.hue_shifter {
            // HueShifter::X | HueShifter::Y => Some(self.update_interval.into()),
            _ => None,
        })
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        match event.button {
            MouseButton::Left => {
                self.current_temp = self.click_temp;
                self.hue_shift_driver.update(self.current_temp)?;
                self.text.set_text(self.current_temp.to_string());
            }
            MouseButton::Right => {
                if self.max_temp > 6500 {
                    self.current_temp = 6500;
                    self.hue_shift_driver.reset()?;
                } else {
                    self.current_temp = self.max_temp;
                    self.hue_shift_driver.update(self.current_temp)?;
                }
                self.text.set_text(self.current_temp.to_string());
            }
            mb => {
                use LogicalDirection::*;
                let new_temp: u16;
                match self.scrolling.to_logical_direction(mb) {
                    Some(Up) => {
                        new_temp = self.current_temp + self.step;
                        if new_temp <= self.max_temp {
                            self.hue_shift_driver.update(new_temp)?;
                            self.current_temp = new_temp;
                        }
                    }
                    Some(Down) => {
                        new_temp = self.current_temp - self.step;
                        if new_temp >= self.min_temp {
                            self.hue_shift_driver.update(new_temp)?;
                            self.current_temp = new_temp;
                        }
                    }
                    None => return Ok(()), // avoid updating text
                }
                self.text.set_text(self.current_temp.to_string());
            }
        }
        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}
