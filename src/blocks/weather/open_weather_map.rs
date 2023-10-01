use super::*;
use chrono::Utc;

pub(super) const URL: &str = "https://api.openweathermap.org/data/2.5/weather";
pub(super) const GEO_URL: &str = "https://api.openweathermap.org/geo/1.0";
pub(super) const API_KEY_ENV: &str = "OPENWEATHERMAP_API_KEY";
pub(super) const CITY_ID_ENV: &str = "OPENWEATHERMAP_CITY_ID";
pub(super) const PLACE_ENV: &str = "OPENWEATHERMAP_PLACE";
pub(super) const ZIP_ENV: &str = "OPENWEATHERMAP_ZIP";

#[derive(Deserialize, Debug)]
#[serde(tag = "name", rename_all = "lowercase")]
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
    #[serde(default = "default_lang")]
    lang: String,
}

pub(super) struct Service<'a> {
    api_key: &'a String,
    units: &'a UnitSystem,
    lang: &'a String,
    location_query: Option<String>,
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
fn default_lang() -> String {
    "en".into()
}

#[derive(Deserialize, Debug)]
struct ApiResponse {
    weather: Vec<ApiWeather>,
    main: ApiMain,
    wind: ApiWind,
    sys: ApiSys,
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
    async fn get_weather(&self, autolocated: Option<Coordinates>) -> Result<WeatherResult> {
        let location_query = autolocated
            .map(|al| format!("lat={}&lon={}", al.latitude, al.longitude))
            .or_else(|| self.location_query.clone())
            .error("no location was provided")?;

        // Refer to https://openweathermap.org/current
        let url = format!(
            "{URL}?{location_query}&appid={api_key}&units={units}&lang={lang}",
            api_key = self.api_key,
            units = match self.units {
                UnitSystem::Metric => "metric",
                UnitSystem::Imperial => "imperial",
            },
            lang = self.lang,
        );

        let data: ApiResponse = REQWEST_CLIENT
            .get(url)
            .send()
            .await
            .error("Forecast request failed")?
            .json()
            .await
            .error("Forecast request failed")?;

        let now = Utc::now().timestamp();
        let is_night = data.sys.sunrise >= now || now >= data.sys.sunset;

        Ok(WeatherResult {
            location: data.name,
            temp: data.main.temp,
            apparent: data.main.feels_like,
            humidity: data.main.humidity,
            weather: data.weather[0].main.clone(),
            weather_verbose: data.weather[0].description.clone(),
            wind: data.wind.speed,
            wind_kmh: data.wind.speed
                * match self.units {
                    UnitSystem::Metric => 3.6,
                    UnitSystem::Imperial => 3.6 * 0.447,
                },
            wind_direction: convert_wind_direction(data.wind.deg).into(),
            icon: match data.weather[0].main.as_str() {
                "Clear" => WeatherIcon::Clear { is_night },
                "Rain" | "Drizzle" => WeatherIcon::Rain { is_night },
                "Clouds" => WeatherIcon::Clouds { is_night },
                "Fog" | "Mist" => WeatherIcon::Fog { is_night },
                "Thunderstorm" => WeatherIcon::Thunder { is_night },
                "Snow" => WeatherIcon::Snow,
                _ => WeatherIcon::Default,
            },
        })
    }
}
