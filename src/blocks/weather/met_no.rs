use super::*;

pub type LegendsStore = HashMap<String, LegendsResult>;

#[derive(Deserialize, Debug)]
#[serde(tag = "name", rename_all = "lowercase")]
pub struct Config {
    coordinates: Option<(String, String)>,
    altitude: Option<String>,
    #[serde(default)]
    lang: ApiLanguage,
}

#[derive(Deserialize)]
pub struct LegendsResult {
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

pub async fn get_legend() -> Result<LegendsStore> {
    let res: LegendsStore = REQWEST_CLIENT
        .get(LEGENDS_URL)
        .send()
        .await
        .error("Failed to fetch legend from met.no")?
        .json()
        .await
        .error("Legend replied in unknown format")?;

    Ok(res)
}

fn translate(legend: &LegendsStore, summary: &str, lang: &ApiLanguage) -> String {
    legend
        .get(summary)
        .map(|res| match lang {
            ApiLanguage::English => res.desc_en.as_str(),
            ApiLanguage::NorwegianBokmaal => res.desc_nb.as_str(),
            ApiLanguage::NorwegianNynorsk => res.desc_nn.as_str(),
        })
        .or_else(|| Some(summary))
        .unwrap()
        .into()
}

pub async fn get(
    config: &Config,
    autolocation: &Option<LocationResponse>,
    legend: &LegendsStore,
) -> Result<WeatherResult> {
    let (lat, lon) = autolocation
        .as_ref()
        .map(|loc| loc.as_coordinates())
        .or_else(|| config.coordinates.clone())
        .error("No location given")?;

    let querystr: HashMap<&str, String> = map! {
        "lat" => lat,
        "lon" => lon,
        "altitude" => config.altitude.as_ref().unwrap(); if config.altitude.is_some()
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
    let translated = translate(legend, summary, &config.lang);

    Ok(WeatherResult {
        location: "Unknown".to_string(),
        temp: instant.air_temperature.unwrap_or_default(),
        apparent: None,
        humidity: instant.relative_humidity.unwrap_or_default(),
        weather: translated.clone(),
        weather_verbose: translated,
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
            | "lightsleet" | "lightsleetshowers" | "lightsnow" | "lightsnowshowers" | "sleet"
            | "sleetshowers" | "snow" | "snowshowers" => WeatherIcon::Snow,
            _ => WeatherIcon::Default,
        },
    })
}
