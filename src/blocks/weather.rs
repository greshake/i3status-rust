//! Current weather
//!
//! This block displays local weather and temperature information. In order to use this block, you
//! will need access to a supported weather API service. At the time of writing, OpenWeatherMap is
//! the only supported service.
//!
//! Configuring this block requires configuring a weather service, which may require API keys and
//! other parameters.
//!
//! If using the `autolocate` feature, set the block update interval such that you do not exceed ipapi.co's free daily limit of 1000 hits.
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"$weather $temp"`
//! `service` | The configuration of a weather service (see below). | Yes | None
//! `interval` | Update interval, in seconds. | No | `600`
//! `autolocate` | Gets your location using the ipapi.co IP location service (no API key required). If the API call fails then the block will fallback to `city_id` or `place`. | No | false
//!
//! # OpenWeatherMap Options
//!
//! To use the service you will need a (free) API key.
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `name` | `openweathermap`. | Yes | None
//! `api_key` | Your OpenWeatherMap API key. | Yes | None
//! `city_id` | OpenWeatherMap's ID for the city. | Yes* | None
//! `place` | OpenWeatherMap 'By city name' search query. See [here](https://openweathermap.org/current) | Yes* | None
//! `coordinates` | GPS latitude longitude coordinates as a tuple, example: `["39.236229089090216","9.331730718685696"]`
//! `units` | Either `metric` or `imperial`. | Yes | `metric`
//! `lang` | Language code. See [here](https://openweathermap.org/current#multi). Currently only affects `weather_verbose` key. | No | `en`
//!
//! One of `city_id`, `place` or `coordinates` is required. If more than one are supplied, `city_id` takes precedence over `place` which takes place over `coordinates`.
//!
//! The options `api_key`, `city_id`, `place` can be omitted from configuration,
//! in which case they must be provided in the environment variables
//! `OPENWEATHERMAP_API_KEY`, `OPENWEATHERMAP_CITY_ID`, `OPENWEATHERMAP_PLACE`.
//!
//! # Available Format Keys
//!
//!  Key              | Value                                                              | Type   | Unit
//! ------------------|--------------------------------------------------------------------|--------|-----
//! `location`        | Location name (exact format depends on the service)                | Text   | -
//! `temp`            | Temperature                                                        | Number | degrees
//! `apparent`        | Australian Apparent Temperature                                    | Number | degrees
//! `humidity`        | Humidity                                                           | Number | %
//! `weather`         | Textual brief description of the weather, e.g. "Raining"           | Text   | -
//! `weather_verbose` | Textual verbose description of the weather, e.g. "overcast clouds" | Text   | -
//! `wind`            | Wind speed                                                         | Number | -
//! `wind_kmh`        | Wind speed. The wind speed in km/h                                 | Number | -
//! `direction`       | Wind direction, e.g. "NE"                                          | Text   | -
//!
//! # Example
//!
//! Show detailed weather in San Francisco through the OpenWeatherMap service:
//!
//! ```toml
//! [[block]]
//! block = "weather"
//! format = "$weather ($location) $temp, $wind m/s $direction"
//! service = { name = "openweathermap", api_key = "XXX", city_id = "5398563", units = "metric" }
//! ```
//!
//! # Used Icons
//!
//! - `weather_sun` (when weather is reported as "Clear")
//! - `weather_rain` (when weather is reported as "Rain" or "Drizzle")
//! - `weather_clouds` (when weather is reported as "Clouds", "Fog" or "Mist")
//! - `weather_thunder` (when weather is reported as "Thunderstorm")
//! - `weather_snow` (when weather is reported as "Snow")
//! - `weather_default` (in all other cases)

use super::prelude::*;

const IP_API_URL: &str = "https://ipapi.co/json";

const OPEN_WEATHER_MAP_URL: &str = "https://api.openweathermap.org/data/2.5/weather";
const OPEN_WEATHER_MAP_API_KEY_ENV: &str = "OPENWEATHERMAP_API_KEY";
const OPEN_WEATHER_MAP_CITY_ID_ENV: &str = "OPENWEATHERMAP_CITY_ID";
const OPEN_WEATHER_MAP_PLACE_ENV: &str = "OPENWEATHERMAP_PLACE";

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct WeatherConfig {
    #[serde(default = "default_interval")]
    interval: Seconds,
    #[serde(default)]
    format: FormatConfig,
    service: WeatherService,
    #[serde(default)]
    autolocate: bool,
}

fn default_interval() -> Seconds {
    Seconds::new(600)
}

#[derive(Deserialize, Debug)]
#[serde(tag = "name", rename_all = "lowercase")]
enum WeatherService {
    OpenWeatherMap {
        #[serde(default = "WeatherService::getenv_openweathermap_api_key")]
        api_key: Option<String>,
        #[serde(default = "WeatherService::getenv_openweathermap_city_id")]
        city_id: Option<String>,
        #[serde(default = "WeatherService::getenv_openweathermap_place")]
        place: Option<String>,
        coordinates: Option<(String, String)>,
        #[serde(default)]
        units: UnitSystem,
        #[serde(default = "WeatherService::default_lang")]
        lang: String,
    },
}

impl WeatherService {
    fn getenv_openweathermap_api_key() -> Option<String> {
        std::env::var(OPEN_WEATHER_MAP_API_KEY_ENV)
            .map(Into::into)
            .ok()
    }
    fn getenv_openweathermap_city_id() -> Option<String> {
        std::env::var(OPEN_WEATHER_MAP_CITY_ID_ENV)
            .map(Into::into)
            .ok()
    }
    fn getenv_openweathermap_place() -> Option<String> {
        std::env::var(OPEN_WEATHER_MAP_PLACE_ENV)
            .map(Into::into)
            .ok()
    }
    fn default_lang() -> String {
        "en".into()
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = WeatherConfig::deserialize(config).config_error()?;
    api.set_format(config.format.with_default("$weather $temp")?);

    loop {
        if let Ok(data) = config.service.get(config.autolocate).await {
            let apparent_temp = australian_apparent_temp(
                data.main.temp,
                data.main.humidity,
                data.wind.speed,
                config.service.units(),
            );

            let kmh_wind_speed = data.wind.speed
                * 3.6
                * match config.service.units() {
                    UnitSystem::Metric => 1.0,
                    UnitSystem::Imperial => 0.447,
                };

            let keys = map! {
                "location" => Value::text(data.name),
                "temp" => Value::degrees(data.main.temp),
                "apparent" => Value::degrees(apparent_temp),
                "humidity" => Value::percents(data.main.humidity),
                "weather" => Value::text(data.weather[0].main.clone()),
                "weather_verbose" => Value::text(data.weather[0].description.clone()),
                "wind" => Value::number(data.wind.speed),
                "wind_kmh" => Value::number(kmh_wind_speed),
                "direction" => Value::text(convert_wind_direction(data.wind.deg).into()),
            };

            let icon = match data.weather[0].main.as_str() {
                "Clear" => "weather_sun",
                "Rain" | "Drizzle" => "weather_rain",
                "Clouds" | "Fog" | "Mist" => "weather_clouds",
                "Thunderstorm" => "weather_thunder",
                "Snow" => "weather_snow",
                _ => "weather_default",
            };

            api.set_icon(icon)?;
            api.set_values(keys);
        } else {
            api.set_text("X".into());
        }

        api.flush().await?;

        sleep(config.interval.0).await;
    }
}

#[derive(Deserialize, Debug)]
struct ApiResponse {
    weather: Vec<ApiWeather>,
    main: ApiMain,
    wind: ApiWind,
    name: String,
}

#[derive(Deserialize, Debug)]
struct ApiWind {
    speed: f64,
    deg: Option<f64>,
}

#[derive(Deserialize, Debug)]
struct ApiMain {
    temp: f64,
    humidity: f64,
}

#[derive(Deserialize, Debug)]
struct ApiWeather {
    main: String,
    description: String,
}

impl WeatherService {
    fn units(&self) -> UnitSystem {
        let Self::OpenWeatherMap { units, .. } = self;
        *units
    }

    async fn get(&self, autolocate: bool) -> Result<ApiResponse> {
        let Self::OpenWeatherMap {
            api_key,
            city_id,
            place,
            coordinates,
            units,
            lang,
        } = self;

        let api_key = api_key.as_ref().or_error(|| {
            format!(
                "missing key 'service.api_key' and environment variable {}",
                OPEN_WEATHER_MAP_API_KEY_ENV
            )
        })?;

        let city = if autolocate {
            find_ip_location().await.unwrap_or(None)
        } else {
            None
        };

        let location_query = {
            city.map(|x| format!("q={}", x))
                .or_else(|| city_id.as_ref().map(|x| format!("id={}", x)))
                .or_else(|| place.as_ref().map(|x| format!("q={}", x)))
                .or_else(|| {
                    coordinates
                        .as_ref()
                        .map(|(lat, lon)| format!("lat={}&lon={}", lat, lon))
                })
                .error("no localization was provided")?
        };

        // Refer to https://openweathermap.org/current
        let url = &format!(
            "{}?{}&appid={}&units={}&lang={}",
            OPEN_WEATHER_MAP_URL,
            location_query,
            api_key,
            match *units {
                UnitSystem::Metric => "metric",
                UnitSystem::Imperial => "imperial",
            },
            lang,
        );

        reqwest::get(url)
            .await
            .error("Failed during request for current location")?
            .json()
            .await
            .error("Failed while parsing location API result")
    }
}

#[derive(Derivative, Copy, Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
enum UnitSystem {
    #[derivative(Default)]
    Metric,
    Imperial,
}

// TODO: might be good to allow for different geolocation services to be used, similar to how we have `service` for the weather API
async fn find_ip_location() -> Result<Option<String>> {
    #[derive(Deserialize)]
    struct ApiResponse {
        city: Option<String>,
    }
    REQWEST_CLIENT
        .get(IP_API_URL)
        .send()
        .await
        .error("Failed during request for current location")?
        .json::<ApiResponse>()
        .await
        .error("Failed while parsing location API result")
        .map(|x| x.city)
}

// Compute the Australian Apparent Temperature (AT),
// using the metric formula found on Wikipedia.
// If using imperial units, we must first convert to metric.
fn australian_apparent_temp(
    raw_temp: f64,
    raw_humidity: f64,
    raw_wind_speed: f64,
    units: UnitSystem,
) -> f64 {
    let temp_celsius = match units {
        UnitSystem::Metric => raw_temp,
        UnitSystem::Imperial => (raw_temp - 32.0) * 0.556,
    };

    let exponent = 17.27 * temp_celsius / (237.7 + temp_celsius);
    let water_vapor_pressure = raw_humidity * 0.06105 * exponent.exp();

    let metric_wind_speed = match units {
        UnitSystem::Metric => raw_wind_speed,
        UnitSystem::Imperial => raw_wind_speed * 0.447,
    };

    let metric_apparent_temp =
        temp_celsius + 0.33 * water_vapor_pressure - 0.7 * metric_wind_speed - 4.0;

    match units {
        UnitSystem::Metric => metric_apparent_temp,
        UnitSystem::Imperial => 1.8 * metric_apparent_temp + 32.,
    }
}

// Convert wind direction in azimuth degrees to abbreviation names
fn convert_wind_direction(direction_opt: Option<f64>) -> &'static str {
    match direction_opt {
        Some(direction) => match direction.round() as i64 {
            24..=68 => "NE",
            69..=113 => "E",
            114..=158 => "SE",
            159..=203 => "S",
            204..=248 => "SW",
            249..=293 => "W",
            294..=338 => "NW",
            _ => "N",
        },
        None => "-",
    }
}
