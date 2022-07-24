//! Current weather
//!
//! This block displays local weather and temperature information. In order to use this block, you
//! will need access to a supported weather API service. At the time of writing, OpenWeatherMap and
//! met.no are supported.
//!
//! Configuring this block requires configuring a weather service, which may require API keys and
//! other parameters.
//!
//! If using the `autolocate` feature, set the block update interval such that you do not exceed ipapi.co's free daily limit of 1000 hits.
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `service` | The configuration of a weather service (see below). | **Required**
//! `format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | `"$weather $temp"`
//! `interval` | Update interval, in seconds. | `600`
//! `autolocate` | Gets your location using the ipapi.co IP location service (no API key required). If the API call fails then the block will fallback to `city_id` or `place`. | `false`
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
//! # met.no Options
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `name` | `metno`. | Yes | None
//! `coordinates` | GPS latitude longitude coordinates as a tuple, example: `["39.236229089090216","9.331730718685696"]`
//! `lang` | Language code: `en`, `nn` or `nb` | No | `en`
//! `altitude` | Meters above sea level of the ground | No | Approximated by server
//!
//! Met.no does not support location name or apparent temperature.
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
//! [block.service]
//! name = "openweathermap"
//! api_key = "XXX"
//! city_id = "5398563"
//! units = "metric"
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

mod open_weather_map {
    use super::*;

    pub const OPEN_WEATHER_MAP_URL: &str = "https://api.openweathermap.org/data/2.5/weather";
    pub const OPEN_WEATHER_MAP_API_KEY_ENV: &str = "OPENWEATHERMAP_API_KEY";
    pub const OPEN_WEATHER_MAP_CITY_ID_ENV: &str = "OPENWEATHERMAP_CITY_ID";
    pub const OPEN_WEATHER_MAP_PLACE_ENV: &str = "OPENWEATHERMAP_PLACE";

    pub struct OpenWeatherMapConfig {
        api_key: Option<String>,
        city_id: Option<String>,
        place: Option<String>,
        coordinates: Option<(String, String)>,
        units: UnitSystem,
        lang: String,
    }

    impl TryFrom<&WeatherService> for OpenWeatherMapConfig {
        type Error = Error;

        fn try_from(w: &WeatherService) -> Result<Self, Self::Error> {
            match w {
                WeatherService::OpenWeatherMap {
                    api_key,
                    city_id,
                    place,
                    coordinates,
                    units,
                    lang,
                } => Ok(OpenWeatherMapConfig {
                    api_key: api_key.clone(),
                    city_id: city_id.clone(),
                    place: place.clone(),
                    coordinates: coordinates.clone(),
                    units: *units,
                    lang: lang.clone(),
                }),
                _ => Err(Error::new("Illegal variant")),
            }
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
        feels_like: f64,
        humidity: f64,
    }

    #[derive(Deserialize, Debug)]
    struct ApiWeather {
        main: String,
        description: String,
    }

    pub async fn get(
        config: OpenWeatherMapConfig,
        autolocated: Option<LocationResponse>,
    ) -> Result<WeatherResult> {
        let OpenWeatherMapConfig {
            api_key,
            city_id,
            place,
            coordinates,
            units,
            lang,
        } = config;

        let api_key = api_key.as_ref().or_error(|| {
            format!(
                "missing key 'service.api_key' and environment variable {OPEN_WEATHER_MAP_API_KEY_ENV}",
            )
        })?;

        let city = match autolocated {
            Some(loc) => loc.city,
            None => None,
        };

        let location_query = city
            .map(|c| format!("q={}", c))
            .or_else(|| city_id.as_ref().map(|x| format!("id={}", x)))
            .or_else(|| place.as_ref().map(|x| format!("q={}", x)))
            .or_else(|| {
                coordinates
                    .as_ref()
                    .map(|(lat, lon)| format!("lat={}&lon={}", lat, lon))
            })
            .error("no location was provided")?;

        // Refer to https://openweathermap.org/current
        let url = format!(
            "{OPEN_WEATHER_MAP_URL}?{location_query}&appid={api_key}&units={units}&lang={lang}",
            units = match units {
                UnitSystem::Metric => "metric",
                UnitSystem::Imperial => "imperial",
            },
        );

        let data: ApiResponse = REQWEST_CLIENT
            .get(url)
            .send()
            .await
            .error("Failed during request for current location")?
            .json()
            .await
            .error("Failed while parsing location API result")?;

        Ok(WeatherResult {
            location: data.name,
            temp: data.main.temp,
            apparent: Some(data.main.feels_like),
            humidity: data.main.humidity,
            weather: data.weather[0].main.clone(),
            weather_verbose: data.weather[0].description.clone(),
            wind: data.wind.speed,
            wind_kmh: data.wind.speed
                * match units {
                    UnitSystem::Metric => 3.6,
                    UnitSystem::Imperial => 3.6 * 0.447,
                },
            wind_direction: convert_wind_direction(data.wind.deg).into(),
            icon: match data.weather[0].main.as_str() {
                "Clear" => WeatherIcon::Sun,
                "Rain" | "Drizzle" => WeatherIcon::Rain,
                "Clouds" | "Fog" | "Mist" => WeatherIcon::Clouds,
                "Thunderstorm" => WeatherIcon::Thunder,
                "Snow" => WeatherIcon::Snow,
                _ => WeatherIcon::Default,
            },
        })
    }
}

mod met_no {
    use super::*;

    pub struct Config {
        coordinates: (String, String),
        altitude: Option<String>,
        lang: ApiLanguage,
    }

    impl Config {
        pub fn create(
            autoloc: Option<LocationResponse>,
            coordinates: &Option<(String, String)>,
            altitude: &Option<String>,
            language: &Option<ApiLanguage>,
        ) -> Result<Self> {
            let c = match autoloc {
                Some(loc) => Some((format!("{}", loc.latitude), format!("{}", loc.longitude))),
                None => coordinates.clone(),
            };

            if let Some(coordinates) = c {
                Ok(Config {
                    coordinates,
                    altitude: altitude.clone(),
                    lang: language.to_owned().unwrap_or_default(),
                })
            } else {
                Err(Error::new_format("No location given to weather (Yr)"))
            }
        }
    }

    #[derive(Deserialize)]
    struct LegendsResult {
        desc_en: String,
        desc_nb: String,
        desc_nn: String,
    }

    #[derive(Deserialize, Debug, Clone)]
    pub enum ApiLanguage {
        #[serde(rename = "en")]
        English,
        #[serde(rename = "nn")]
        NorwegianNynorsk,
        #[serde(rename = "nb")]
        NorwegianBokmaal,
    }

    impl Default for ApiLanguage {
        fn default() -> Self {
            ApiLanguage::English
        }
    }

    impl From<&Option<&str>> for ApiLanguage {
        fn from(o: &Option<&str>) -> Self {
            match o.as_deref() {
                Some(s) => match s {
                    "nn" => ApiLanguage::NorwegianNynorsk,
                    "nb" => ApiLanguage::NorwegianBokmaal,
                    _ => ApiLanguage::English,
                },
                None => ApiLanguage::English,
            }
        }
    }

    #[derive(Deserialize, Debug)]
    struct ForecastResponse {
        properties: ForecastProperties,
    }

    #[derive(Deserialize, Debug)]
    struct ForecastProperties {
        timeseries: Vec<ForecastTimeStep>,
    }

    #[derive(Deserialize, Debug)]
    struct ForecastTimeStep {
        data: ForecastData,
        // time: String,
    }

    #[derive(Deserialize, Debug)]
    struct ForecastData {
        instant: ForecastModelInstant,
        // next_12_hours: ForecastModelPeriod,
        next_1_hours: Option<ForecastModelPeriod>,
        // next_6_hours: ForecastModelPeriod,
    }

    #[derive(Deserialize, Debug)]
    struct ForecastModelInstant {
        details: ForecastTimeInstant,
    }

    #[derive(Deserialize, Debug)]
    struct ForecastModelPeriod {
        summary: ForecastSummary,
    }

    #[derive(Deserialize, Debug)]
    struct ForecastSummary {
        symbol_code: String,
    }

    #[derive(Deserialize, Debug, Default)]
    struct ForecastTimeInstant {
        air_temperature: Option<f64>,
        wind_from_direction: Option<f64>,
        wind_speed: Option<f64>,
        relative_humidity: Option<f64>,
    }

    const LEGENDS_URL: &str = "https://api.met.no/weatherapi/weathericon/2.0/legends";
    const FORECAST_URL: &str = "https://api.met.no/weatherapi/locationforecast/2.0/compact";

    async fn translate(summary: &str, lang: ApiLanguage) -> Result<String> {
        let legend: HashMap<String, LegendsResult> = REQWEST_CLIENT
            .get(LEGENDS_URL)
            .send()
            .await
            .error("Failed to fetch legend from met.no")?
            .json()
            .await
            .error("Legend replied in unknown format")?;

        let default_result = LegendsResult {
            desc_en: summary.into(),
            desc_nb: summary.into(),
            desc_nn: summary.into(),
        };

        let data = legend.get(summary).unwrap_or(&default_result);
        Ok(match lang {
            ApiLanguage::English => data.desc_en.clone(),
            ApiLanguage::NorwegianBokmaal => data.desc_nb.clone(),
            ApiLanguage::NorwegianNynorsk => data.desc_nn.clone(),
        })
    }

    pub async fn get(config: Config) -> Result<WeatherResult> {
        let querystr: HashMap<&str, String> = map! {
            "lat" => config.coordinates.0,
            "lon" => config.coordinates.1,
            "altitude" => config.altitude.unwrap_or_default(); if config.altitude.is_some()
        };

        let data: ForecastResponse = REQWEST_CLIENT
            .get(FORECAST_URL)
            .query(&querystr)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .send()
            .await
            .error("Failed during request for current location")?
            .json()
            .await
            .error("Failed while parsing location API result")?;

        let first = &data.properties.timeseries.first().unwrap().data;

        let instant = &first.instant.details;

        let summary = first
            .next_1_hours
            .as_ref()
            .unwrap()
            .summary
            .symbol_code
            .split('_')
            .next()
            .unwrap();
        let verbose = translate(summary, config.lang).await?;

        Ok(WeatherResult {
            location: "Unknown".to_string(),
            temp: instant.air_temperature.unwrap_or_default(),
            apparent: None,
            humidity: instant.relative_humidity.unwrap_or_default(),
            weather: verbose.clone(),
            weather_verbose: verbose,
            wind: instant.wind_speed.unwrap_or_default(),
            wind_kmh: instant.wind_speed.unwrap_or_default() * 3.6,
            wind_direction: convert_wind_direction(instant.wind_from_direction).into(),
            icon: match summary {
                "cloudy" | "partlycloudy" | "fair" | "fog" => WeatherIcon::Clouds,
                "clearsky" => WeatherIcon::Sun,
                "heavyrain" | "heavyrainshowers" | "lightrain" | "lightrainshowers" | "rain"
                | "rainshowers" => WeatherIcon::Rain,
                "rainandthunder"
                | "heavyrainandthunder"
                | "rainshowersandthunder"
                | "sleetandthunder"
                | "sleetshowersandthunder"
                | "snowandthunder"
                | "snowshowersandthunder"
                | "heavyrainshowersandthunder"
                | "heavysleetandthunder"
                | "heavysleetshowersandthunder"
                | "heavysnowandthunder"
                | "heavysnowshowersandthunder"
                | "lightsleetandthunder"
                | "lightrainandthunder"
                | "lightsnowandthunder"
                | "lightssleetshowersandthunder"
                | "lightssnowshowersandthunder"
                | "lightrainshowersandthunder" => WeatherIcon::Thunder,
                "heavysleet" | "heavysleetshowers" | "heavysnow" | "heavysnowshowers"
                | "lightsleet" | "lightsleetshowers" | "lightsnow" | "lightsnowshowers"
                | "sleet" | "sleetshowers" | "snow" | "snowshowers" => WeatherIcon::Snow,
                _ => WeatherIcon::Default,
            },
        })
    }
}

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
pub enum WeatherService {
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
    MetNo {
        coordinates: Option<(String, String)>,
        altitude: Option<String>,
        #[serde(default)]
        lang: Option<met_no::ApiLanguage>,
    },
}

impl WeatherService {
    fn getenv_openweathermap_api_key() -> Option<String> {
        std::env::var(open_weather_map::OPEN_WEATHER_MAP_API_KEY_ENV).ok()
    }
    fn getenv_openweathermap_city_id() -> Option<String> {
        std::env::var(open_weather_map::OPEN_WEATHER_MAP_CITY_ID_ENV).ok()
    }
    fn getenv_openweathermap_place() -> Option<String> {
        std::env::var(open_weather_map::OPEN_WEATHER_MAP_PLACE_ENV).ok()
    }
    fn default_lang() -> String {
        "en".into()
    }

    async fn get(&self, autolocated_location: Option<LocationResponse>) -> Result<WeatherResult> {
        match self {
            WeatherService::OpenWeatherMap { .. } => {
                open_weather_map::get(self.try_into()?, autolocated_location).await
            }
            WeatherService::MetNo {
                coordinates,
                altitude,
                lang,
            } => {
                met_no::get(met_no::Config::create(
                    autolocated_location,
                    coordinates,
                    altitude,
                    lang,
                )?)
                .await
            }
        }
    }
}
pub enum WeatherIcon {
    Sun,
    Rain,
    Clouds,
    Thunder,
    Snow,
    Default,
}

impl WeatherIcon {
    fn to_icon_str(&self) -> &str {
        match self {
            WeatherIcon::Sun => "weather_sun",
            WeatherIcon::Rain => "weather_rain",
            WeatherIcon::Clouds => "weather_clouds",
            WeatherIcon::Thunder => "weather_thunder",
            WeatherIcon::Snow => "weather_snow",
            WeatherIcon::Default => "weather_default",
        }
    }
}

pub struct WeatherResult {
    pub location: String,
    pub temp: f64,
    pub apparent: Option<f64>,
    pub humidity: f64,
    pub weather: String,
    pub weather_verbose: String,
    pub wind: f64,
    pub wind_kmh: f64,
    pub wind_direction: String,
    pub icon: WeatherIcon,
}

impl WeatherResult {
    fn values(&self) -> HashMap<Cow<'static, str>, Value> {
        map! {
            "location" => Value::text(self.location.clone()),
            "temp" => Value::degrees(self.temp),
            "apparent" => Value::degrees(self.apparent.unwrap_or_default()),
            "humidity" => Value::percents(self.humidity),
            "weather" => Value::text(self.weather.clone()),
            "weather_verbose" => Value::text(self.weather_verbose.clone()),
            "wind" => Value::number(self.wind),
            "wind_kmh" => Value::number(self.wind_kmh),
            "direction" => Value::text(self.wind_direction.clone()),
        }
    }
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = WeatherConfig::deserialize(config).config_error()?;
    let mut widget = api
        .new_widget()
        .with_format(config.format.with_default("$weather $temp")?);

    loop {
        let location = match config.autolocate {
            true => find_ip_location().await.unwrap_or(None),
            false => None,
        };

        let data = api
            .recoverable(|| config.service.get(location.clone()))
            .await?;

        widget.set_values(data.values());
        widget.set_icon(data.icon.to_icon_str())?;
        api.set_widget(&widget).await?;

        select! {
            _ = sleep(config.interval.0) => (),
            _ = api.wait_for_update_request() => (),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, SmartDefault)]
#[serde(rename_all = "lowercase")]
pub enum UnitSystem {
    #[default]
    Metric,
    Imperial,
}

#[derive(Deserialize, Clone)]
pub struct LocationResponse {
    city: Option<String>,
    latitude: f64,
    longitude: f64,
}

// TODO: might be good to allow for different geolocation services to be used, similar to how we have `service` for the weather API
async fn find_ip_location() -> Result<Option<LocationResponse>> {
    REQWEST_CLIENT
        .get(IP_API_URL)
        .send()
        .await
        .error("Failed during request for current location")?
        .json()
        .await
        .error("Failed while parsing location API result")
}

// Convert wind direction in azimuth degrees to abbreviation names
pub fn convert_wind_direction(direction_opt: Option<f64>) -> &'static str {
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
