use super::*;

type LegendsStore = HashMap<String, LegendsResult>;

#[derive(Deserialize, Debug)]
#[serde(tag = "name", rename_all = "lowercase")]
pub struct Config {
    coordinates: Option<(String, String)>,
    altitude: Option<String>,
    #[serde(default)]
    lang: ApiLanguage,
}

pub(super) struct Service<'a> {
    config: &'a Config,
    legend: &'static LegendsStore,
}

impl<'a> Service<'a> {
    pub(super) async fn new(config: &'a Config) -> Result<Service<'a>> {
        Ok(Self {
            config,
            legend: LEGENDS.as_ref().error("Invalid legends file")?,
        })
    }
}

#[derive(Deserialize)]
struct LegendsResult {
    desc_en: String,
    desc_nb: String,
    desc_nn: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub(super) enum ApiLanguage {
    #[serde(rename = "en")]
    #[default]
    English,
    #[serde(rename = "nn")]
    NorwegianNynorsk,
    #[serde(rename = "nb")]
    NorwegianBokmaal,
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

static LEGENDS: Lazy<Option<LegendsStore>> =
    Lazy::new(|| serde_json::from_str(include_str!("met_no_legends.json")).ok());

const FORECAST_URL: &str = "https://api.met.no/weatherapi/locationforecast/2.0/compact";

fn translate(legend: &LegendsStore, summary: &str, lang: &ApiLanguage) -> String {
    legend
        .get(summary)
        .map(|res| match lang {
            ApiLanguage::English => res.desc_en.as_str(),
            ApiLanguage::NorwegianBokmaal => res.desc_nb.as_str(),
            ApiLanguage::NorwegianNynorsk => res.desc_nn.as_str(),
        })
        .unwrap_or(summary)
        .into()
}

#[async_trait]
impl WeatherProvider for Service<'_> {
    async fn get_weather(&self, location: Option<Coordinates>) -> Result<WeatherResult> {
        let Config {
            coordinates,
            altitude,
            lang,
        } = &self.config;

        let (lat, lon) = location
            .as_ref()
            .map(|loc| (loc.latitude.to_string(), loc.longitude.to_string()))
            .or_else(|| coordinates.clone())
            .error("No location given")?;

        let querystr: HashMap<&str, String> = map! {
            "lat" => lat,
            "lon" => lon,
            [if let Some(alt) = altitude] "altitude" => alt,
        };

        let data: ForecastResponse = REQWEST_CLIENT
            .get(FORECAST_URL)
            .query(&querystr)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .send()
            .await
            .error("Forecast request failed")?
            .json()
            .await
            .error("Forecast request failed")?;

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
        let translated = translate(self.legend, summary, lang);

        let temp = instant.air_temperature.unwrap_or_default();
        let humidity = instant.relative_humidity.unwrap_or_default();
        let wind_speed = instant.wind_speed.unwrap_or_default();

        Ok(WeatherResult {
            location: "Unknown".to_string(),
            temp,
            apparent: australian_apparent_temp(temp, humidity, wind_speed),
            humidity,
            weather: translated.clone(),
            weather_verbose: translated,
            wind: wind_speed,
            wind_kmh: instant.wind_speed.unwrap_or_default() * 3.6,
            wind_direction: convert_wind_direction(instant.wind_from_direction).into(),
            icon: weather_to_icon(summary),
        })
    }
}

fn weather_to_icon(weather: &str) -> WeatherIcon {
    match weather {
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
        "heavysleet" | "heavysleetshowers" | "heavysnow" | "heavysnowshowers" | "lightsleet"
        | "lightsleetshowers" | "lightsnow" | "lightsnowshowers" | "sleet" | "sleetshowers"
        | "snow" | "snowshowers" => WeatherIcon::Snow,
        _ => WeatherIcon::Default,
    }
}
