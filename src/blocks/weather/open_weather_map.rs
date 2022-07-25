use super::*;

pub const URL: &str = "https://api.openweathermap.org/data/2.5/weather";
pub const API_KEY_ENV: &str = "OPENWEATHERMAP_API_KEY";
pub const CITY_ID_ENV: &str = "OPENWEATHERMAP_CITY_ID";
pub const PLACE_ENV: &str = "OPENWEATHERMAP_PLACE";

#[derive(Deserialize, Debug)]
#[serde(tag = "name", rename_all = "lowercase")]
pub struct Config {
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

pub async fn get(config: &Config, autolocated: &Option<LocationResponse>) -> Result<WeatherResult> {
    let Config {
        api_key,
        city_id,
        place,
        coordinates,
        units,
        lang,
    } = config;

    let api_key = api_key.as_ref().or_error(|| {
        format!("missing key 'service.api_key' and environment variable {API_KEY_ENV}",)
    })?;

    // If autolocated is Some, and then if autolocated.city is Some
    let city = match autolocated {
        Some(loc) => loc.city.as_ref(),
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
        "{URL}?{location_query}&appid={api_key}&units={units}&lang={lang}",
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
