use super::*;

pub(super) const URL: &str = "https://api.openweathermap.org/data/2.5/weather";
pub(super) const API_KEY_ENV: &str = "OPENWEATHERMAP_API_KEY";
pub(super) const CITY_ID_ENV: &str = "OPENWEATHERMAP_CITY_ID";
pub(super) const PLACE_ENV: &str = "OPENWEATHERMAP_PLACE";

#[derive(Deserialize, Debug)]
#[serde(tag = "name", rename_all = "lowercase")]
pub(super) struct Config {
    #[serde(default = "getenv_openweathermap_api_key")]
    api_key: Option<String>,
    #[serde(default = "getenv_openweathermap_city_id")]
    city_id: Option<String>,
    #[serde(default = "getenv_openweathermap_place")]
    place: Option<String>,
    coordinates: Option<(String, String)>,
    #[serde(default)]
    units: UnitSystem,
    #[serde(default = "default_lang")]
    lang: String,
}

pub(super) struct Service {
    config: Config,
}

impl Service {
    pub(super) fn new(config: Config) -> Self {
        Self { config }
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
fn default_lang() -> String {
    "en".into()
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

#[async_trait]
impl WeatherProvider for Service {
    async fn get_weather(&self, autolocated: Option<Coordinates>) -> Result<WeatherResult> {
        let api_key = self.config.api_key.as_ref().or_error(|| {
            format!("missing key 'service.api_key' and environment variable {API_KEY_ENV}",)
        })?;

        let location_query = autolocated
            .map(|al| format!("lat={}&lon={}", al.latitude, al.longitude))
            .or_else(|| {
                self.config
                    .coordinates
                    .as_ref()
                    .map(|(lat, lon)| format!("lat={lat}&lon={lon}"))
            })
            .or_else(|| self.config.city_id.as_ref().map(|x| format!("id={}", x)))
            .or_else(|| self.config.place.as_ref().map(|x| format!("q={}", x)))
            .error("no location was provided")?;

        // Refer to https://openweathermap.org/current
        let url = format!(
            "{URL}?{location_query}&appid={api_key}&units={units}&lang={lang}",
            units = match self.config.units {
                UnitSystem::Metric => "metric",
                UnitSystem::Imperial => "imperial",
            },
            lang = self.config.lang,
        );

        let data: ApiResponse = REQWEST_CLIENT
            .get(url)
            .send()
            .await
            .error("Forecast request failed")?
            .json()
            .await
            .error("Forecast request failed")?;

        Ok(WeatherResult {
            location: data.name,
            temp: data.main.temp,
            apparent: data.main.feels_like,
            humidity: data.main.humidity,
            weather: data.weather[0].main.clone(),
            weather_verbose: data.weather[0].description.clone(),
            wind: data.wind.speed,
            wind_kmh: data.wind.speed
                * match self.config.units {
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
