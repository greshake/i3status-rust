#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "name", rename_all = "lowercase")]
pub enum WeatherService {
    // TODO:
    // DarkSky {
    //     token: String,
    //     latitude: f64,
    //     longitude: f64,
    //     units: Option<InputUnit>
    // },
    OpenWeatherMap {
        api_key: String,
        city_id: String,
        units: OpenWeatherMapUnits,
    }
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenWeatherMapUnits {
    Metric,
    Imperial,
}

#[cfg(not(feature = "darksky"))]
pub use self::openweathermap::*;

#[cfg(feature = "darksky")]
pub use self::darksky::*;

mod openweathermap {
    use std::process::Command;
    use std::time::Duration;
    use chan::Sender;
    use serde_json;
    use uuid::Uuid;

    use block::{Block, ConfigBlock};
    use config::Config;
    use de::deserialize_duration;
    use errors::*;
    use widgets::text::TextWidget;
    use widget::I3BarWidget;
    use scheduler::Task;

    use super::{WeatherService, OpenWeatherMapUnits};

    pub struct Weather {
        id: String,
        weather: TextWidget,
        service: WeatherService,
        update_interval: Duration,
    }

    impl Weather {
        fn update_weather(&mut self) -> Result<()> {
            match self.service {
                WeatherService::OpenWeatherMap { ref api_key, ref city_id, ref units } => {
                    let json: serde_json::value::Value = Command::new("sh")
                        .args(
                            &[
                                "-c",
                                &format!(
                                    "curl \"http://api.openweathermap.org/data/2.5/weather?id={city_id}&appid={api_key}&units={units}\"",
                                    city_id = city_id,
                                    api_key = api_key,
                                    units = match *units {
                                        OpenWeatherMapUnits::Metric => "metric",
                                        OpenWeatherMapUnits::Imperial => "imperial",
                                    },
                                ),
                            ],
                        )
                        .output()
                        .block_error("weather", "Failed to exectute curl.")
                        .and_then(|raw_output| String::from_utf8(raw_output.stdout).block_error("weather", "Non-UTF8 SSID."))
                        .and_then(|output| serde_json::from_str(&output).block_error("weather", "Failed to parse JSON response."))?;

                    // Try to convert an API error into a block error.
                    if let Some(val) = json.get("message") {
                        return Err(BlockError("weather".to_string(), format!("API Error: {}", val.as_str().unwrap())));
                    };
                    let raw_weather = match json.pointer("/weather/0/main")
                        .and_then(|value| value.as_str())
                        .map(|s| s.to_string())
                    {
                        Some(v) => v,
                        None => {
                            return Err(BlockError("weather".to_string(),
                                                  "Malformed JSON.".to_string()));
                        }
                    };
                    let raw_temp = match json.pointer("/main/temp")
                        .and_then(|value| value.as_f64())
                    {
                        Some(v) => v,
                        None => {
                            return Err(BlockError("weather".to_string(),
                                                  "Malformed JSON.".to_string()));
                        }
                    };

                    self.weather.set_icon(match raw_weather.as_str() {
                        "Clear" => "weather_sun",
                        "Rain" | "Drizzle" => "weather_rain",
                        "Clouds" | "Fog" | "Mist" => "weather_clouds",
                        "Thunderstorm" => "weather_thunder",
                        "Snow" => "weather_snow",
                        _ => "weather_default",
                    });
                    self.weather.set_text(format!(" {} {:.1}\u{00b0}", raw_weather, raw_temp));
                    Ok(())
                }
            }
        }
    }

    #[derive(Deserialize, Debug, Clone)]
    #[serde(deny_unknown_fields)]
    pub struct WeatherConfig {
        #[serde(default = "WeatherConfig::default_interval", deserialize_with = "deserialize_duration")]
        pub interval: Duration,
        pub service: WeatherService,
    }

    impl WeatherConfig {
        fn default_interval() -> Duration {
            Duration::from_secs(600)
        }
    }

    impl ConfigBlock for Weather {
        type Config = WeatherConfig;

        fn new(block_config: Self::Config, config: Config, _tx_update_request: Sender<Task>) -> Result<Self> {
            Ok(Weather {
                id: Uuid::new_v4().simple().to_string(),
                weather: TextWidget::new(config),
                service: block_config.service,
                update_interval: block_config.interval,
            })
        }
    }

    impl Block for Weather {
        fn update(&mut self) -> Result<Option<Duration>> {
            self.update_weather()?;
            Ok(Some(self.update_interval))
        }

        fn view(&self) -> Vec<&I3BarWidget> {
            vec![&self.weather]
        }

        fn id(&self) -> &str {
            &self.id
        }
    }
}

#[cfg(feature = "darksky")]
mod darksky {
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
        forecast: Arc<Mutex<Option<darksky::Forecast>>>,
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
}
