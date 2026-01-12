use super::*;
use chrono::{DateTime, Utc};
use reqwest::Url;
use serde::{Deserializer, de};

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
    location_query: Option<LocationSpecifier>,
    forecast_hours: usize,
}

#[derive(Clone)]
enum LocationSpecifier {
    CityCoord(CityCoord),
    LocationId(u32),
}

impl LocationSpecifier {
    fn as_query_params(&self) -> Vec<(&str, String)> {
        match self {
            LocationSpecifier::CityCoord(city) => {
                vec![("lat", city.lat.to_string()), ("lon", city.lon.to_string())]
            }
            LocationSpecifier::LocationId(id) => vec![("id", id.to_string())],
        }
    }
}

fn parse_coord(value: &str, name: &str) -> Result<f64, Error> {
    value
        .parse::<f64>()
        .or_error(|| format!("Invalid {} '{}': expected an f64", name, value))
}

impl TryFrom<(&String, &String)> for LocationSpecifier {
    type Error = Error;

    fn try_from(coords: (&String, &String)) -> Result<Self, Self::Error> {
        let lat = parse_coord(coords.0, "latitude")?;
        let lon = parse_coord(coords.1, "longitude")?;

        Ok(LocationSpecifier::CityCoord(CityCoord { lat, lon }))
    }
}

impl TryFrom<&String> for LocationSpecifier {
    type Error = Error;

    fn try_from(id: &String) -> Result<Self, Self::Error> {
        let id = id
            .parse::<u32>()
            .or_error(|| format!("Invalid city id '{}': expected a u32", id))?;

        Ok(LocationSpecifier::LocationId(id))
    }
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
        api_key: &str,
        config: &Config,
    ) -> Result<Option<LocationSpecifier>> {
        if autolocate {
            return Ok(None);
        }

        // Try by coordinates from config
        if let Some((lat, lon)) = config.coordinates.as_ref() {
            return Ok(Some(LocationSpecifier::try_from((lat, lon)).error(
                "Invalid coordinates: failed to parse latitude or longitude from string to f64",
            )?));
        }

        // Try by city ID from config
        if let Some(id) = config.city_id.as_ref() {
            return Ok(Some(LocationSpecifier::try_from(id).error(
                "Invalid city id: failed to parse it from string to u32",
            )?));
        }

        let geo_url =
            Url::parse(GEO_URL).error("Failed to parse the hard-coded constant GEO_URL")?;

        // Try by place name
        if let Some(place) = config.place.as_ref() {
            // "{GEO_URL}/direct?q={place}&appid={api_key}"
            let mut url = geo_url.join("direct").error("Failed to join geo_url")?;
            url.query_pairs_mut()
                .append_pair("q", place)
                .append_pair("appid", api_key);

            let city: Option<LocationSpecifier> = REQWEST_CLIENT
                .get(url)
                .send()
                .await
                .error("Geo request failed")?
                .json::<Vec<CityCoord>>()
                .await
                .error("Geo failed to parse JSON")?
                .first()
                .map(|city| LocationSpecifier::CityCoord(*city));

            return Ok(city);
        }

        // Try by zip code
        if let Some(zip) = config.zip.as_ref() {
            // "{GEO_URL}/zip?zip={zip}&appid={api_key}"
            let mut url = geo_url.join("zip").error("Failed to join geo_url")?;
            url.query_pairs_mut()
                .append_pair("zip", zip)
                .append_pair("appid", api_key);

            let city: CityCoord = REQWEST_CLIENT
                .get(url)
                .send()
                .await
                .error("Geo request failed")?
                .json()
                .await
                .error("Geo failed to parse JSON")?;

            return Ok(Some(LocationSpecifier::CityCoord(city)));
        }

        Ok(None)
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

impl ApiInstantResponse {
    fn wind_kmh(&self, units: &UnitSystem) -> f64 {
        self.wind.speed
            * match units {
                UnitSystem::Metric => 3.6,
                UnitSystem::Imperial => 3.6 * 0.447,
            }
    }

    fn to_moment(&self, units: &UnitSystem, current_data: &ApiCurrentResponse) -> WeatherMoment {
        let is_night = current_data.sys.sunrise >= self.dt || self.dt >= current_data.sys.sunset;

        WeatherMoment {
            icon: weather_to_icon(self.weather[0].main.as_str(), is_night),
            weather: self.weather[0].main.clone(),
            weather_verbose: self.weather[0].description.clone(),
            temp: self.main.temp,
            apparent: self.main.feels_like,
            humidity: self.main.humidity,
            wind: self.wind.speed,
            wind_kmh: self.wind_kmh(units),
            wind_direction: self.wind.deg,
        }
    }

    fn to_aggregate(&self, units: &UnitSystem) -> ForecastAggregateSegment {
        ForecastAggregateSegment {
            temp: Some(self.main.temp),
            apparent: Some(self.main.feels_like),
            humidity: Some(self.main.humidity),
            wind: Some(self.wind.speed),
            wind_kmh: Some(self.wind_kmh(units)),
            wind_direction: self.wind.deg,
        }
    }
}

#[derive(Deserialize, Debug)]
struct ApiCurrentResponse {
    #[serde(flatten)]
    instant: ApiInstantResponse,
    sys: ApiSys,
    name: String,
}

impl ApiCurrentResponse {
    fn to_moment(&self, units: &UnitSystem) -> WeatherMoment {
        self.instant.to_moment(units, self)
    }
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

#[derive(Deserialize, Debug, Copy, Clone)]
struct CityCoord {
    lat: f64,
    lon: f64,
}

#[async_trait]
impl WeatherProvider for Service<'_> {
    async fn get_weather(
        &self,
        autolocated: Option<&IPAddressInfo>,
        need_forecast: bool,
    ) -> Result<WeatherResult> {
        let location_specifier = autolocated
            .as_ref()
            .map(|al| {
                LocationSpecifier::CityCoord(CityCoord {
                    lat: al.latitude,
                    lon: al.longitude,
                })
            })
            .or_else(|| self.location_query.clone())
            .error("no location was provided")?;

        // Refer to https://openweathermap.org/current
        let current_url =
            Url::parse(CURRENT_URL).error("Failed to parse the hard-coded constant CURRENT_URL")?;

        let common_query_params = [
            ("appid", self.api_key.as_str()),
            ("units", self.units.as_ref()),
            ("lang", self.lang.as_str()),
        ];

        // "{CURRENT_URL}?{location_query}&appid={api_key}&units={units}&lang={lang}"
        let current_data: ApiCurrentResponse = REQWEST_CLIENT
            .get(current_url)
            .query(&location_specifier.as_query_params())
            .query(&common_query_params)
            .send()
            .await
            .error("Current weather request failed")?
            .json()
            .await
            .error("Current weather request failed")?;

        let current_weather = current_data.to_moment(self.units);

        let sunrise = Some(
            DateTime::<Utc>::from_timestamp(current_data.sys.sunrise, 0)
                .error("Unable to convert timestamp to DateTime")?,
        );

        let sunset = Some(
            DateTime::<Utc>::from_timestamp(current_data.sys.sunset, 0)
                .error("Unable to convert timestamp to DateTime")?,
        );

        if !need_forecast || self.forecast_hours == 0 {
            return Ok(WeatherResult {
                location: current_data.name,
                current_weather,
                forecast: None,
                sunrise,
                sunset,
            });
        }

        // Refer to https://openweathermap.org/forecast5
        let forecast_url = Url::parse(FORECAST_URL)
            .error("Failed to parse the hard-coded constant FORECAST_URL")?;

        let forecast_query_params = [("cnt", &(self.forecast_hours / 3).to_string())];

        // "{FORECAST_URL}?{location_query}&appid={api_key}&units={units}&lang={lang}&cnt={cnt}",
        let forecast_data: ApiForecastResponse = REQWEST_CLIENT
            .get(forecast_url)
            .query(&location_specifier.as_query_params())
            .query(&common_query_params)
            .query(&forecast_query_params)
            .send()
            .await
            .error("Forecast weather request failed")?
            .json()
            .await
            .error("Forecast weather request failed")?;

        let data_agg: Vec<ForecastAggregateSegment> = forecast_data
            .list
            .iter()
            .take(self.forecast_hours)
            .map(|f| f.to_aggregate(self.units))
            .collect();

        let fin = forecast_data
            .list
            .last()
            .error("no weather available")?
            .to_moment(self.units, &current_data);

        let forecast = Some(Forecast::new(&data_agg, fin));

        Ok(WeatherResult {
            location: current_data.name,
            current_weather,
            forecast,
            sunrise,
            sunset,
        })
    }
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
