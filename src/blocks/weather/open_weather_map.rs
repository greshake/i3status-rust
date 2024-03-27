use std::time::UNIX_EPOCH;

use super::*;
use chrono::{DateTime, Utc};
use serde::{de, Deserializer};

pub(super) const GEO_URL: &str = "https://api.openweathermap.org/geo/1.0";
pub(super) const CURRENT_URL: &str = "https://api.openweathermap.org/data/2.5/weather";
pub(super) const FORECAST_URL: &str = "https://api.openweathermap.org/data/2.5/forecast";
pub(super) const API_KEY_ENV: &str = "OPENWEATHERMAP_API_KEY";
pub(super) const CITY_ID_ENV: &str = "OPENWEATHERMAP_CITY_ID";
pub(super) const PLACE_ENV: &str = "OPENWEATHERMAP_PLACE";
pub(super) const ZIP_ENV: &str = "OPENWEATHERMAP_ZIP";

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(tag = "name", rename_all = "lowercase", deny_unknown_fields, default)]
pub struct Config {
    #[serde(default = "getenv_openweathermap_api_key")]
    api_key: Option<String>,
    #[serde(default = "getenv_openweathermap_city_id")]
    city_id: Option<String>,
    #[serde(default = "getenv_openweathermap_place")]
    place: Option<String>,
    #[serde(default = "getenv_openweathermap_zip")]
    zip: Option<String>,
    coordinates: Option<(String, String)>,
    #[serde(default)]
    units: UnitSystem,
    #[default("en")]
    lang: String,
    #[default(12)]
    #[serde(deserialize_with = "deserialize_forecast_hours")]
    forecast_hours: usize,
}

pub fn deserialize_forecast_hours<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    usize::deserialize(deserializer).and_then(|hours| {
        if hours % 3 != 0 && hours > 120 {
            Err(de::Error::custom(
                "'forecast_hours' is not divisible by 3 and must be <= 120",
            ))
        } else if hours % 3 != 0 {
            Err(de::Error::custom("'forecast_hours' is not divisible by 3"))
        } else if hours > 120 {
            Err(de::Error::custom("'forecast_hours' must be <= 120"))
        } else {
            Ok(hours)
        }
    })
}

pub(super) struct Service<'a> {
    api_key: &'a String,
    units: &'a UnitSystem,
    lang: &'a String,
    location_query: Option<String>,
    forecast_hours: usize,
}

impl<'a> Service<'a> {
    pub(super) async fn new(autolocate: bool, config: &'a Config) -> Result<Service<'a>> {
        let api_key = config.api_key.as_ref().or_error(|| {
            format!("missing key 'service.api_key' and environment variable {API_KEY_ENV}",)
        })?;
        Ok(Self {
            api_key,
            units: &config.units,
            lang: &config.lang,
            location_query: Service::get_location_query(autolocate, api_key, config).await?,
            forecast_hours: config.forecast_hours,
        })
    }

    async fn get_location_query(
        autolocate: bool,
        api_key: &String,
        config: &Config,
    ) -> Result<Option<String>> {
        if autolocate {
            return Ok(None);
        }

        let mut location_query = config
            .coordinates
            .as_ref()
            .map(|(lat, lon)| format!("lat={lat}&lon={lon}"))
            .or_else(|| config.city_id.as_ref().map(|x| format!("id={x}")));

        location_query = match location_query {
            Some(x) => Some(x),
            None => match config.place.as_ref() {
                Some(place) => {
                    let url = format!("{GEO_URL}/direct?q={place}&appid={api_key}");

                    REQWEST_CLIENT
                        .get(url)
                        .send()
                        .await
                        .error("Geo request failed")?
                        .json::<Vec<CityCoord>>()
                        .await
                        .error("Geo failed to parse json")?
                        .first()
                        .map(|city| format!("lat={}&lon={}", city.lat, city.lon))
                }
                None => None,
            },
        };

        location_query = match location_query {
            Some(x) => Some(x),
            None => match config.zip.as_ref() {
                Some(zip) => {
                    let url = format!("{GEO_URL}/zip?zip={zip}&appid={api_key}");
                    let city: CityCoord = REQWEST_CLIENT
                        .get(url)
                        .send()
                        .await
                        .error("Geo request failed")?
                        .json()
                        .await
                        .error("Geo failed to parse json")?;

                    Some(format!("lat={}&lon={}", city.lat, city.lon))
                }
                None => None,
            },
        };

        Ok(location_query)
    }
}

fn getenv_openweathermap_api_key() -> Option<String> {
    std::env::var(API_KEY_ENV).ok()
}
fn getenv_openweathermap_city_id() -> Option<String> {
    std::env::var(CITY_ID_ENV).ok()
}
fn getenv_openweathermap_place() -> Option<String> {
    std::env::var(PLACE_ENV).ok()
}
fn getenv_openweathermap_zip() -> Option<String> {
    std::env::var(ZIP_ENV).ok()
}

#[derive(Deserialize, Debug)]
struct ApiForecastResponse {
    list: Vec<ApiInstantResponse>,
}

#[derive(Deserialize, Debug)]
struct ApiInstantResponse {
    weather: Vec<ApiWeather>,
    main: ApiMain,
    wind: ApiWind,
    dt: i64,
}

#[derive(Deserialize, Debug)]
struct ApiCurrentResponse {
    weather: Vec<ApiWeather>,
    main: ApiMain,
    wind: ApiWind,
    sys: ApiSys,
    name: String,
    dt: i64,
}

#[derive(Deserialize, Debug)]
struct ApiWind {
    speed: f64,
    deg: Option<f64>,
}

#[derive(Deserialize, Debug)]
struct ApiMain {
    temp: f64,
    feels_like: f64,
    humidity: f64,
}

#[derive(Deserialize, Debug)]
struct ApiSys {
    sunrise: i64,
    sunset: i64,
}

#[derive(Deserialize, Debug)]
struct ApiWeather {
    main: String,
    description: String,
}

#[derive(Deserialize, Debug)]
struct CityCoord {
    lat: f64,
    lon: f64,
}

#[async_trait]
impl WeatherProvider for Service<'_> {
    async fn get_weather(
        &self,
        autolocated: Option<&Coordinates>,
        need_forecast: bool,
    ) -> Result<WeatherResult> {
        let location_query = autolocated
            .as_ref()
            .map(|al| format!("lat={}&lon={}", al.latitude, al.longitude))
            .or_else(|| self.location_query.clone())
            .error("no location was provided")?;

        // Refer to https://openweathermap.org/current
        let current_url = format!(
            "{CURRENT_URL}?{location_query}&appid={api_key}&units={units}&lang={lang}",
            api_key = self.api_key,
            units = match self.units {
                UnitSystem::Metric => "metric",
                UnitSystem::Imperial => "imperial",
            },
            lang = self.lang,
        );

        let current_data: ApiCurrentResponse = REQWEST_CLIENT
            .get(current_url)
            .send()
            .await
            .error("Current weather request failed")?
            .json()
            .await
            .error("Current weather request failed")?;

        let current_weather = {
            let is_night = current_data.sys.sunrise >= current_data.dt
                || current_data.dt >= current_data.sys.sunset;
            WeatherMoment {
                temp: current_data.main.temp,
                apparent: current_data.main.feels_like,
                humidity: current_data.main.humidity,
                weather: current_data.weather[0].main.clone(),
                weather_verbose: current_data.weather[0].description.clone(),
                wind: current_data.wind.speed,
                wind_kmh: current_data.wind.speed
                    * match self.units {
                        UnitSystem::Metric => 3.6,
                        UnitSystem::Imperial => 3.6 * 0.447,
                    },
                wind_direction: current_data.wind.deg,
                icon: weather_to_icon(current_data.weather[0].main.as_str(), is_night),
            }
        };

        let forecast = if !need_forecast || self.forecast_hours == 0 {
            None
        } else {
            // Refer to https://openweathermap.org/forecast5
            let forecast_url = format!(
                "{FORECAST_URL}?{location_query}&appid={api_key}&units={units}&lang={lang}&cnt={cnt}",
                api_key = self.api_key,
                units = match self.units {
                    UnitSystem::Metric => "metric",
                    UnitSystem::Imperial => "imperial",
                },
                lang = self.lang,
                cnt = self.forecast_hours / 3,
            );

            let forecast_data: ApiForecastResponse = REQWEST_CLIENT
                .get(forecast_url)
                .send()
                .await
                .error("Forecast weather request failed")?
                .json()
                .await
                .error("Forecast weather request failed")?;

            let mut temp_avg = 0.0;
            let mut temp_min = f64::MAX;
            let mut temp_max = f64::MIN;
            let mut apparent_avg = 0.0;
            let mut apparent_min = f64::MAX;
            let mut apparent_max = f64::MIN;
            let mut humidity_avg = 0.0;
            let mut humidity_min = f64::MAX;
            let mut humidity_max = f64::MIN;
            let mut wind_forecasts = Vec::new();
            let mut forecast_count = 0.0;
            for forecast_instant in &forecast_data.list {
                let instant_main = &forecast_instant.main;
                temp_avg += instant_main.temp;
                temp_min = temp_min.min(instant_main.temp);
                temp_max = temp_max.max(instant_main.temp);
                apparent_avg += instant_main.feels_like;
                apparent_min = apparent_min.min(instant_main.feels_like);
                apparent_max = apparent_max.max(instant_main.feels_like);
                humidity_avg += instant_main.humidity;
                humidity_min = humidity_min.min(instant_main.humidity);
                humidity_max = humidity_max.max(instant_main.humidity);
                forecast_count += 1.0;

                let instant_wind = &forecast_instant.wind;
                wind_forecasts.push(Wind {
                    speed: instant_wind.speed,
                    degrees: instant_wind.deg,
                });
            }
            temp_avg /= forecast_count;
            apparent_avg /= forecast_count;
            humidity_avg /= forecast_count;
            let Wind {
                speed: wind_avg,
                degrees: direction_avg,
            } = average_wind(&wind_forecasts);
            let Wind {
                speed: wind_min,
                degrees: direction_min,
            } = wind_forecasts
                .iter()
                .min_by(|x, y| x.speed.total_cmp(&y.speed))
                .error("No min wind")?;
            let Wind {
                speed: wind_max,
                degrees: direction_max,
            } = wind_forecasts
                .iter()
                .min_by(|x, y| x.speed.total_cmp(&y.speed))
                .error("No max wind")?;

            let fin_data = forecast_data.list.last().unwrap();
            let fin_is_night =
                current_data.sys.sunrise >= fin_data.dt || fin_data.dt >= current_data.sys.sunset;

            Some(Forecast {
                avg: ForecastAggregate {
                    temp: temp_avg,
                    apparent: apparent_avg,
                    humidity: humidity_avg,
                    wind: wind_avg,
                    wind_kmh: wind_avg
                        * match self.units {
                            UnitSystem::Metric => 3.6,
                            UnitSystem::Imperial => 3.6 * 0.447,
                        },
                    wind_direction: direction_avg,
                },
                min: ForecastAggregate {
                    temp: temp_min,
                    apparent: apparent_min,
                    humidity: humidity_min,
                    wind: *wind_min,
                    wind_kmh: wind_min
                        * match self.units {
                            UnitSystem::Metric => 3.6,
                            UnitSystem::Imperial => 3.6 * 0.447,
                        },
                    wind_direction: *direction_min,
                },
                max: ForecastAggregate {
                    temp: temp_max,
                    apparent: apparent_max,
                    humidity: humidity_max,
                    wind: *wind_max,
                    wind_kmh: wind_max
                        * match self.units {
                            UnitSystem::Metric => 3.6,
                            UnitSystem::Imperial => 3.6 * 0.447,
                        },
                    wind_direction: *direction_max,
                },
                fin: WeatherMoment {
                    icon: weather_to_icon(fin_data.weather[0].main.as_str(), fin_is_night),
                    weather: fin_data.weather[0].main.clone(),
                    weather_verbose: fin_data.weather[0].description.clone(),
                    temp: fin_data.main.temp,
                    apparent: fin_data.main.feels_like,
                    humidity: fin_data.main.humidity,
                    wind: fin_data.wind.speed,
                    wind_kmh: fin_data.wind.speed
                        * match self.units {
                            UnitSystem::Metric => 3.6,
                            UnitSystem::Imperial => 3.6 * 0.447,
                        },
                    wind_direction: fin_data.wind.deg,
                },
            })
        };

        let sunrise = unix_to_datetime(current_data.sys.sunrise);
        let sunset = unix_to_datetime(current_data.sys.sunset);

        Ok(WeatherResult {
            location: current_data.name,
            current_weather,
            forecast,
            sunrise,
            sunset,
        })
    }
}

fn unix_to_datetime(timestamp: i64) -> DateTime<Utc> {
    let d = UNIX_EPOCH + Duration::from_secs(timestamp.unsigned_abs());
    // Create DateTime from SystemTime
    DateTime::<Utc>::from(d)
}

fn weather_to_icon(weather: &str, is_night: bool) -> WeatherIcon {
    match weather {
        "Clear" => WeatherIcon::Clear { is_night },
        "Rain" | "Drizzle" => WeatherIcon::Rain { is_night },
        "Clouds" => WeatherIcon::Clouds { is_night },
        "Fog" | "Mist" => WeatherIcon::Fog { is_night },
        "Thunderstorm" => WeatherIcon::Thunder { is_night },
        "Snow" => WeatherIcon::Snow,
        _ => WeatherIcon::Default,
    }
}
