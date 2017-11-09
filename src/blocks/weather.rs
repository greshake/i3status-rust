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
    },
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenWeatherMapUnits {
    Metric,
    Imperial,
}

pub struct Weather {
    id: String,
    weather: TextWidget,
    service: WeatherService,
    update_interval: Duration,
}

impl Weather {
    fn update_weather(&mut self) -> Result<()> {
        match self.service {
            WeatherService::OpenWeatherMap {
                ref api_key,
                ref city_id,
                ref units,
            } => {
                let output = Command::new("sh")
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
                    .and_then(|raw_output| {
                        String::from_utf8(raw_output.stdout).block_error("weather", "Non-UTF8 SSID.")
                    })?;

                let json: serde_json::value::Value = serde_json::from_str(&output).block_error(
                    "weather",
                    "Failed to parse JSON response.",
                )?;

                // Try to convert an API error into a block error.
                if let Some(val) = json.get("message") {
                    return Err(BlockError(
                        "weather".to_string(),
                        format!("API Error: {}", val.as_str().unwrap()),
                    ));
                };
                let raw_weather = match json.pointer("/weather/0/main")
                    .and_then(|value| value.as_str())
                    .map(|s| s.to_string()) {
                    Some(v) => v,
                    None => {
                        return Err(BlockError(
                            "weather".to_string(),
                            "Malformed JSON.".to_string(),
                        ));
                    }
                };
                let raw_temp = match json.pointer("/main/temp").and_then(|value| value.as_f64()) {
                    Some(v) => v,
                    None => {
                        return Err(BlockError(
                            "weather".to_string(),
                            "Malformed JSON.".to_string(),
                        ));
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
                self.weather.set_text(format!(
                    " {} {:.1}\u{00b0}",
                    raw_weather,
                    raw_temp
                ));
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
