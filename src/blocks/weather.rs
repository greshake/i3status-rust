use darksky::{self, Unit, Icon};
use serde_json::{self, Value};
use std::boxed::Box;
use std::error::Error;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

use widget::I3BarWidget;
use widgets::text::TextWidget;
use scheduler::Task;
use block::Block;

const DEFAULT_UPDATE_INTERVAL: u64 = 30;
const DEFAULT_UNIT: InputUnit = InputUnit::Si;

pub struct Weather {
    id: String,
    widget: TextWidget,
    config: Config,
    forecast: Arc<Mutex<Option<darksky::Forecast>>>
}

#[derive(Clone, Deserialize)]
enum InputUnit {
    #[serde(rename = "SI")]
    Si,
    #[serde(rename = "US")]
    Us
}

impl InputUnit {
    fn to_str(&self) -> &'static str {
        match self {
            &InputUnit::Si => "℃",
            &InputUnit::Us => "℉"
        }
    }
}

impl From<InputUnit> for Unit {
    fn from(unit: InputUnit) -> Unit {
        match unit {
            InputUnit::Si => Unit::Si,
            InputUnit::Us => Unit::Us
        }
    }
}

#[derive(Clone, Deserialize)]
struct Config {
    token: String,
    latitude: f64,
    longitude: f64,
    interval: Option<u64>,
    units: Option<InputUnit>
}

impl Weather {
    pub fn new(config: Value, send: Sender<Task>, theme: Value) -> Weather {
        let id: String = Uuid::new_v4().simple().to_string();
        let id_clone = id.clone();
        
        let config: Config = match serde_json::from_value(config) {
            Ok(config) => config,
            Err(error) => panic!("Error in weather block configuration: {}",
                                 error.description())
        };
        let config_clone = config.clone();

        let forecast = Arc::new(Mutex::new(None));
        let forecast_clone = forecast.clone();

        let widget = TextWidget::new(theme);

        thread::spawn(move || {
            let id = id_clone;
            let config = config_clone;
            let forecast = forecast_clone;
            let units = config.units.unwrap_or(DEFAULT_UNIT);
            loop {
                let set_units = |options: darksky::Options| {
                    options.unit(units.clone().into())
                };
                if let Ok(data) =
                    darksky::get_forecast_with_options(config.token.clone(),
                                                       config.latitude,
                                                       config.longitude,
                                                       set_units) {
                    let mut forecast = forecast.lock().unwrap();
                    *forecast = Some(data);
                    send.send(Task {
                        id: id.clone(),
                        update_time: Instant::now()
                    }).unwrap();
                }
                thread::sleep(
                    Duration::new(config.interval
                                  .unwrap_or(DEFAULT_UPDATE_INTERVAL) * 60, 0));
            }
        });

        Weather {
            id,
            widget,
            config,
            forecast
        }
    }

    fn to_string(&self) -> Option<String> {
        let temp = if let Ok(ref forecast) = self.forecast.try_lock() {
            forecast.as_ref()
                .and_then(|forecast| forecast.currently.as_ref())
                .and_then(|datapoint| datapoint.temperature)
        } else {
            return None;
        };
        let suffix = self.config.units
            .as_ref()
            .unwrap_or(&DEFAULT_UNIT)
            .to_str();
        temp.map(|temp| {
            format!("{:.1}{}", temp, suffix)
        })
    }

    fn icon_name(&self) -> Option<&'static str> {
        let icon = if let Ok(ref forecast) = self.forecast.try_lock() {
            forecast.as_ref()
                .and_then(|forecast| forecast.currently.as_ref())
                .and_then(|datapoint| datapoint.icon)
        } else {
            return None;
        };
        icon.and_then(|icon| {
            // Font Awesome has a limited set of weather icons available so we
            // combine similar phenomena
            match icon {
                Icon::ClearDay => Some("weather_sun"),
                Icon::Snow | Icon::Hail => Some("weather_snow"),
                Icon::Thunderstorm => Some("weather_thunder"),
                Icon::Cloudy | Icon::PartlyCloudyDay | Icon::PartlyCloudyNight => Some("weather_clouds"),
                Icon::Rain => Some("weather_rain"),
                _ => None
            }
        })
    }
}

impl Block for Weather {
    fn id(&self) -> &str {
        &self.id
    }

    fn update(&mut self) -> Option<Duration> {
        let icon = self.icon_name().unwrap_or("weather_default");
        self.widget.set_icon(icon);
        let string = self.to_string().unwrap_or("-".to_owned());
        self.widget.set_text(string);
        None
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.widget]
    }
}
