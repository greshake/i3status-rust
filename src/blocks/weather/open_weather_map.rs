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
