//! Current weather
//!
//! This block displays local weather and temperature information. In order to use this block, you
//! will need access to a supported weather API service. At the time of writing, OpenWeatherMap,
//! met.no, and the US National Weather Service are supported.
//!
//! Configuring this block requires configuring a weather service, which may require API keys and
//! other parameters.
//!
//! If using the `autolocate` feature, set the autolocate update interval such that you do not exceed ipapi.co's free daily limit of 1000 hits. Or use `autolocate_interval = "once"` to only run on initialization.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `service` | The configuration of a weather service (see below). | **Required**
//! `format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | `" $icon $weather $temp "`
//! `format_alt` | If set, block will switch between `format` and `format_alt` on every click | `None`
//! `interval` | Update interval, in seconds. | `600`
//! `autolocate` | Gets your location using the ipapi.co IP location service (no API key required). If the API call fails then the block will fallback to service specific location config. | `false`
//! `autolocate_interval` | Update interval for `autolocate` in seconds or "once" | `interval`
//!
//! # OpenWeatherMap Options
//!
//! To use the service you will need a (free) API key.
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `name` | `openweathermap`. | Yes | None
//! `api_key` | Your OpenWeatherMap API key. | Yes | None
//! `coordinates` | GPS latitude longitude coordinates as a tuple, example: `["39.2362","9.3317"]` | Yes* | None
//! `city_id` | OpenWeatherMap's ID for the city. (Deprecated) | Yes* | None
//! `place` | OpenWeatherMap 'By {city name},{state code},{country code}' search query. See [here](https://openweathermap.org/api/geocoding-api#direct_name). Consumes an additional API call | Yes* | None
//! `zip` | OpenWeatherMap 'By {zip code},{country code}' search query. See [here](https://openweathermap.org/api/geocoding-api#direct_zip). Consumes an additional API call | Yes* | None
//! `units` | Either `"metric"` or `"imperial"`. | No | `"metric"`
//! `lang` | Language code. See [here](https://openweathermap.org/current#multi). Currently only affects `weather_verbose` key. | No | `"en"`
//! `forecast_hours` | How many hours should be forecast (must be increments of 3 hours, max 120 hours) | No | 12
//!
//! One of `coordinates`, `city_id`, `place`, or `zip` is required. If more than one are supplied, `coordinates` takes precedence over `city_id` which takes precedence over `place` which takes precedence over `zip`.
//!
//! The options `api_key`, `city_id`, `place`, `zip`, can be omitted from configuration,
//! in which case they must be provided in the environment variables
//! `OPENWEATHERMAP_API_KEY`, `OPENWEATHERMAP_CITY_ID`, `OPENWEATHERMAP_PLACE`, `OPENWEATHERMAP_ZIP`.
//!
//! Forecasts are only fetched if forecast_hours > 0 and the format has keys related to forecast.
//!
//! # met.no Options
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `name` | `metno`. | Yes | None
//! `coordinates` | GPS latitude longitude coordinates as a tuple, example: `["39.2362","9.3317"]` | Required if `autolocate = false` | None
//! `lang` | Language code: `en`, `nn` or `nb` | No | `en`
//! `altitude` | Meters above sea level of the ground | No | Approximated by server
//! `forecast_hours` | How many hours should be forecast | No | 12
//!
//! Met.no does not support location name, but if autolocate is enabled then autolocate's city value is used.
//!
//! # NWS Options
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `name` | `nws`. | Yes | None
//! `coordinates` | GPS latitude longitude coordinates as a tuple, example: `["39.2362","9.3317"]` | Required if `autolocate = false` | None
//! `forecast_hours` | How many hours should be forecast | No | 12
//! `units` | Either `"metric"` or `"imperial"`. | No | `"metric"`
//!
//! Location name support is not included,
//!
//! # Available Format Keys
//!
//!  Key                                         | Value                                                                         | Type   | Unit
//! ---------------------------------------------|-------------------------------------------------------------------------------|--------|-----
//! `location`                                   | Location name (exact format depends on the service)                           | Text   | -
//! `icon{,_ffin}`                               | Icon representing the weather                                                 | Icon   | -
//! `weather{,_ffin}`                            | Textual brief description of the weather, e.g. "Raining"                      | Text   | -
//! `weather_verbose{,_ffin}`                    | Textual verbose description of the weather, e.g. "overcast clouds"            | Text   | -
//! `temp{,_{favg,fmin,fmax,ffin}}`              | Temperature                                                                   | Number | degrees
//! `apparent{,_{favg,fmin,fmax,ffin}}`          | Australian Apparent Temperature                                               | Number | degrees
//! `humidity{,_{favg,fmin,fmax,ffin}}`          | Humidity                                                                      | Number | %
//! `wind{,_{favg,fmin,fmax,ffin}}`              | Wind speed                                                                    | Number | -
//! `wind_kmh{,_{favg,fmin,fmax,ffin}}`          | Wind speed. The wind speed in km/h                                            | Number | -
//! `direction{,_{favg,fmin,fmax,ffin}}`         | Wind direction, e.g. "NE"                                                     | Text   | -
//!
//! You can use the suffixes noted above to get the following:
//!
//! Suffix    | Description
//! ----------|------------
//! None      | Current weather
//! `_favg`   | Average forecast value
//! `_fmin`   | Minimum forecast value
//! `_fmax`   | Maximum forecast value
//! `_ffin`   | Final forecast value
//!
//! Action          | Description                               | Default button
//! ----------------|-------------------------------------------|---------------
//! `toggle_format` | Toggles between `format` and `format_alt` | Left
//!
//! # Example
//!
//! Show detailed weather in San Francisco through the OpenWeatherMap service:
//!
//! ```toml
//! [[block]]
//! block = "weather"
//! format = " $icon $weather ($location) $temp, $wind m/s $direction "
//! format_alt = " $icon_ffin Forecast (9 hour avg) {$temp_favg ({$temp_fmin}-{$temp_fmax})|Unavailable} "
//! [block.service]
//! name = "openweathermap"
//! api_key = "XXX"
//! city_id = "5398563"
//! units = "metric"
//! forecast_hours = 9
//! ```
//!
//! # Used Icons
//!
//! - `weather_sun` (when weather is reported as "Clear" during the day)
//! - `weather_moon` (when weather is reported as "Clear" at night)
//! - `weather_clouds` (when weather is reported as "Clouds" during the day)
//! - `weather_clouds_night` (when weather is reported as "Clouds" at night)
//! - `weather_fog` (when weather is reported as "Fog" or "Mist" during the day)
//! - `weather_fog_night` (when weather is reported as "Fog" or "Mist" at night)
//! - `weather_rain` (when weather is reported as "Rain" or "Drizzle" during the day)
//! - `weather_rain_night` (when weather is reported as "Rain" or "Drizzle" at night)
//! - `weather_snow` (when weather is reported as "Snow")
//! - `weather_thunder` (when weather is reported as "Thunderstorm" during the day)
//! - `weather_thunder_night` (when weather is reported as "Thunderstorm" at night)

use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::formatting::Format;

use super::prelude::*;

pub mod met_no;
pub mod open_weather_map;
pub mod nws;

const IP_API_URL: &str = "https://ipapi.co/json";

static LAST_AUTOLOCATE: Mutex<Option<AutolocateResult>> = Mutex::new(None);

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_interval")]
    pub interval: Seconds,
    #[serde(default)]
    pub format: FormatConfig,
    pub format_alt: Option<FormatConfig>,
    pub service: WeatherService,
    #[serde(default)]
    pub autolocate: bool,
    pub autolocate_interval: Option<Seconds>,
}

fn default_interval() -> Seconds {
    Seconds::new(600)
}

#[async_trait]
trait WeatherProvider {
    async fn get_weather(
        &self,
        autolocated_location: Option<&Coordinates>,
        need_forecast: bool,
    ) -> Result<WeatherResult>;
}

#[derive(Deserialize, Debug)]
#[serde(tag = "name", rename_all = "lowercase")]
pub enum WeatherService {
    OpenWeatherMap(open_weather_map::Config),
    MetNo(met_no::Config),
    Nws(nws::Config),
}

#[derive(Clone, Copy)]
enum WeatherIcon {
    Clear { is_night: bool },
    Clouds { is_night: bool },
    Fog { is_night: bool },
    Rain { is_night: bool },
    Snow,
    Thunder { is_night: bool },
    Default,
}

impl WeatherIcon {
    fn to_icon_str(self) -> &'static str {
        match self {
            Self::Clear { is_night: false } => "weather_sun",
            Self::Clear { is_night: true } => "weather_moon",
            Self::Clouds { is_night: false } => "weather_clouds",
            Self::Clouds { is_night: true } => "weather_clouds_night",
            Self::Fog { is_night: false } => "weather_fog",
            Self::Fog { is_night: true } => "weather_fog_night",
            Self::Rain { is_night: false } => "weather_rain",
            Self::Rain { is_night: true } => "weather_rain_night",
            Self::Snow => "weather_snow",
            Self::Thunder { is_night: false } => "weather_thunder",
            Self::Thunder { is_night: true } => "weather_thunder_night",
            Self::Default => "weather_default",
        }
    }
}

#[derive(Debug)]
struct Wind {
    speed: f64,
    degrees: Option<f64>,
}

impl PartialEq for Wind {
    fn eq(&self, other: &Self) -> bool {
        (self.speed - other.speed).abs() < 0.001
            && match (self.degrees, other.degrees) {
                (Some(degrees0), Some(degrees1)) => (degrees0 - degrees1).abs() < 0.001,
                (None, None) => true,
                _ => false,
            }
    }
}

struct WeatherMoment {
    icon: WeatherIcon,
    weather: String,
    weather_verbose: String,
    temp: f64,
    apparent: f64,
    humidity: f64,
    wind: f64,
    wind_kmh: f64,
    wind_direction: Option<f64>,
}
struct ForecastAggregate {
    temp: f64,
    apparent: f64,
    humidity: f64,
    wind: f64,
    wind_kmh: f64,
    wind_direction: Option<f64>,
}

struct WeatherResult {
    location: String,
    current_weather: WeatherMoment,
    forecast: Option<Forecast>,
}

struct Forecast {
    avg: ForecastAggregate,
    min: ForecastAggregate,
    max: ForecastAggregate,
    fin: WeatherMoment,
}

impl WeatherResult {
    fn into_values(self) -> Values {
        let mut values = map! {
            "location" => Value::text(self.location),
            //current_weather
            "icon" => Value::icon(self.current_weather.icon.to_icon_str()),
            "temp" => Value::degrees(self.current_weather.temp),
            "apparent" => Value::degrees(self.current_weather.apparent),
            "humidity" => Value::percents(self.current_weather.humidity),
            "weather" => Value::text(self.current_weather.weather),
            "weather_verbose" => Value::text(self.current_weather.weather_verbose),
            "wind" => Value::number(self.current_weather.wind),
            "wind_kmh" => Value::number(self.current_weather.wind_kmh),
            "direction" => Value::text(convert_wind_direction(self.current_weather.wind_direction).into()),
        };

        if let Some(forecast) = self.forecast {
            macro_rules! map_forecasts {
                ({$($suffix: literal => $src: expr),* $(,)?}) => {
                    map!{ @extend values
                        $(
                            concat!("temp_f", $suffix) => Value::degrees($src.temp),
                            concat!("apparent_f", $suffix) => Value::degrees($src.apparent),
                            concat!("humidity_f", $suffix) => Value::percents($src.humidity),
                            concat!("wind_f", $suffix) => Value::number($src.wind),
                            concat!("wind_kmh_f", $suffix) => Value::number($src.wind_kmh),
                            concat!("direction_f", $suffix) => Value::text(convert_wind_direction($src.wind_direction).into()),
                        )*
                    }
                };
            }
            map_forecasts!({
                "avg" => forecast.avg,
                "min" => forecast.min,
                "max" => forecast.max,
                "fin" => forecast.fin,
            });

            map! { @extend values
                "icon_ffin" => Value::icon(forecast.fin.icon.to_icon_str()),
                "weather_ffin" => Value::text(forecast.fin.weather.clone()),
                "weather_verbose_ffin" => Value::text(forecast.fin.weather_verbose.clone()),
            }
        }
        values
    }
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[(MouseButton::Left, None, "toggle_format")])?;

    let mut format = config.format.with_default(" $icon $weather $temp ")?;
    let mut format_alt = match &config.format_alt {
        Some(f) => Some(f.with_default("")?),
        None => None,
    };

    let provider: Box<dyn WeatherProvider + Send + Sync> = match &config.service {
        WeatherService::MetNo(service_config) => Box::new(met_no::Service::new(service_config)?),
        WeatherService::OpenWeatherMap(service_config) => {
            Box::new(open_weather_map::Service::new(config.autolocate, service_config).await?)
        },
        WeatherService::Nws(service_config) => {
            Box::new(nws::Service::new(config.autolocate, service_config).await?)
        },
    };

    let autolocate_interval = config.autolocate_interval.unwrap_or(config.interval);
    let need_forecast = need_forecast(&format, format_alt.as_ref());

    let mut timer = config.interval.timer();

    loop {
        let location = if config.autolocate {
            let fetch = || find_ip_location(autolocate_interval.0);
            Some(fetch.retry(&ExponentialBuilder::default()).await?)
        } else {
            None
        };

        let fetch = || provider.get_weather(location.as_ref(), need_forecast);
        let data = fetch.retry(&ExponentialBuilder::default()).await?;
        let data_values = data.into_values();

        loop {
            let mut widget = Widget::new().with_format(format.clone());
            widget.set_values(data_values.clone());
            api.set_widget(widget)?;

            select! {
                _ = timer.tick() => break,
                _ = api.wait_for_update_request() => break,
                Some(action) = actions.recv() => match action.as_ref() {
                        "toggle_format" => {
                            if let Some(ref mut format_alt) = format_alt {
                                std::mem::swap(format_alt, &mut format);
                            }
                        }
                        _ => (),
                    }
            }
        }
    }
}

fn need_forecast(format: &Format, format_alt: Option<&Format>) -> bool {
    fn has_forecast_key(format: &Format) -> bool {
        macro_rules! format_suffix {
            ($($suffix: literal),* $(,)?) => {
                false
                $(
                    || format.contains_key(concat!("temp_f", $suffix))
                    || format.contains_key(concat!("apparent_f", $suffix))
                    || format.contains_key(concat!("humidity_f", $suffix))
                    || format.contains_key(concat!("wind_f", $suffix))
                    || format.contains_key(concat!("wind_kmh_f", $suffix))
                    || format.contains_key(concat!("direction_f", $suffix))
                )*
            };
        }

        format_suffix!("avg", "min", "max", "fin")
            || format.contains_key("icon_ffin")
            || format.contains_key("weather_ffin")
            || format.contains_key("weather_verbose_ffin")
    }
    has_forecast_key(format) || format_alt.is_some_and(has_forecast_key)
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, SmartDefault)]
#[serde(rename_all = "lowercase")]
enum UnitSystem {
    #[default]
    Metric,
    Imperial,
}

#[derive(Deserialize, Clone)]
struct Coordinates {
    latitude: f64,
    longitude: f64,
    city: String,
}

struct AutolocateResult {
    location: Coordinates,
    timestamp: Instant,
}

// TODO: might be good to allow for different geolocation services to be used, similar to how we have `service` for the weather API
/// No-op if last API call was made in the last `interval` seconds.
async fn find_ip_location(interval: Duration) -> Result<Coordinates> {
    {
        let guard = LAST_AUTOLOCATE.lock().unwrap();
        if let Some(cached) = &*guard {
            if cached.timestamp.elapsed() < interval {
                return Ok(cached.location.clone());
            }
        }
    }

    #[derive(Deserialize)]
    struct ApiResponse {
        #[serde(flatten)]
        location: Option<Coordinates>,
        #[serde(default)]
        error: bool,
        #[serde(default)]
        reason: ApiError,
    }

    #[derive(Deserialize, Default, Debug)]
    #[serde(transparent)]
    struct ApiError(Option<String>);

    impl fmt::Display for ApiError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0.as_deref().unwrap_or("Unknown Error"))
        }
    }
    impl StdError for ApiError {}

    let response: ApiResponse = REQWEST_CLIENT
        .get(IP_API_URL)
        .send()
        .await
        .error("Failed during request for current location")?
        .json()
        .await
        .error("Failed while parsing location API result")?;

    let location = if response.error {
        return Err(Error {
            message: Some("ipapi.co error".into()),
            cause: Some(Arc::new(response.reason)),
        });
    } else {
        response
            .location
            .error("Failed while parsing location API result")?
    };

    {
        let mut guard = LAST_AUTOLOCATE.lock().unwrap();
        *guard = Some(AutolocateResult {
            location: location.clone(),
            timestamp: Instant::now(),
        });
    }

    Ok(location)
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

// Compute the average wind speed and direction
fn average_wind(winds: &[Wind]) -> Wind {
    let mut north = 0.0;
    let mut east = 0.0;
    let mut count = 0.0;
    for wind in winds {
        if let Some(degrees) = wind.degrees {
            let (sin, cos) = degrees.to_radians().sin_cos();
            north += wind.speed * cos;
            east += wind.speed * sin;
            count += 1.0;
        }
    }
    if count == 0.0 {
        Wind {
            speed: 0.0,
            degrees: None,
        }
    } else {
        Wind {
            speed: east.hypot(north) / count,
            degrees: Some(east.atan2(north).to_degrees().rem_euclid(360.0)),
        }
    }
}

/// Compute the Australian Apparent Temperature from metric units
fn australian_apparent_temp(temp: f64, humidity: f64, wind_speed: f64) -> f64 {
    let exponent = 17.27 * temp / (237.7 + temp);
    let water_vapor_pressure = humidity * 0.06105 * exponent.exp();
    temp + 0.33 * water_vapor_pressure - 0.7 * wind_speed - 4.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_average_wind_speed() {
        let mut degrees = 0.0;
        while degrees < 360.0 {
            let averaged = average_wind(&[
                Wind {
                    speed: 1.0,
                    degrees: Some(degrees),
                },
                Wind {
                    speed: 2.0,
                    degrees: Some(degrees),
                },
            ]);
            assert_eq!(
                averaged,
                Wind {
                    speed: 1.5,
                    degrees: Some(degrees)
                }
            );

            degrees += 15.0;
        }
    }

    #[test]
    fn test_average_wind_degrees() {
        let mut degrees = 0.0;
        while degrees < 360.0 {
            let low = degrees - 1.0;
            let high = degrees + 1.0;
            let averaged = average_wind(&[
                Wind {
                    speed: 1.0,
                    degrees: Some(low),
                },
                Wind {
                    speed: 1.0,
                    degrees: Some(high),
                },
            ]);
            // For winds of equal strength the direction should will be the
            // average of the low and high degrees
            assert!((averaged.degrees.unwrap() - degrees).abs() < 0.1);

            degrees += 15.0;
        }
    }

    #[test]
    fn test_average_wind_speed_and_degrees() {
        let mut degrees = 0.0;
        while degrees < 360.0 {
            let low = degrees - 1.0;
            let high = degrees + 1.0;
            let averaged = average_wind(&[
                Wind {
                    speed: 1.0,
                    degrees: Some(low),
                },
                Wind {
                    speed: 2.0,
                    degrees: Some(high),
                },
            ]);
            // Wind degree will be higher than the centerpoint of the low
            // and high winds since the high wind is stronger and will be
            // less than high
            // (low+high)/2 < average.degrees < high
            assert!((low + high) / 2.0 < averaged.degrees.unwrap());
            assert!(averaged.degrees.unwrap() < high);
            degrees += 15.0;
        }
    }
}
