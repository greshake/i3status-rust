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
            | "lightsleet" | "lightsleetshowers" | "lightsnow" | "lightsnowshowers" | "sleet"
            | "sleetshowers" | "snow" | "snowshowers" => WeatherIcon::Snow,
            _ => WeatherIcon::Default,
        },
    })
}
