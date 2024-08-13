use super::*;

type LegendsStore = HashMap<String, LegendsResult>;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(tag = "name", rename_all = "lowercase", deny_unknown_fields, default)]
pub struct Config {
    coordinates: Option<(String, String)>,
    altitude: Option<String>,
    #[serde(default)]
    lang: ApiLanguage,
    #[default(12)]
    forecast_hours: usize,
}

pub(super) struct Service<'a> {
    config: &'a Config,
    legend: &'static LegendsStore,
}

impl<'a> Service<'a> {
    pub(super) fn new(config: &'a Config) -> Result<Service<'a>> {
        Ok(Self {
            config,
            legend: LEGENDS.as_ref().error("Invalid legends file")?,
        })
    }

    fn translate(&self, summary: &str) -> String {
        self.legend
            .get(summary)
            .map(|res| match self.config.lang {
                ApiLanguage::English => res.desc_en.as_str(),
                ApiLanguage::NorwegianBokmaal => res.desc_nb.as_str(),
                ApiLanguage::NorwegianNynorsk => res.desc_nn.as_str(),
            })
            .unwrap_or(summary)
            .into()
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

impl ForecastTimeStep {
    fn to_moment(&self, service: &Service) -> WeatherMoment {
        let instant = &self.data.instant.details;

        let mut symbol_code_split = self
            .data
            .next_1_hours
            .as_ref()
            .unwrap()
            .summary
            .symbol_code
            .split('_');

        let summary = symbol_code_split.next().unwrap();

        // Times of day can be day, night, and polartwilight
        let is_night = symbol_code_split
            .next()
            .map_or(false, |time_of_day| time_of_day == "night");

        let translated = service.translate(summary);

        let temp = instant.air_temperature.unwrap_or_default();
        let humidity = instant.relative_humidity.unwrap_or_default();
        let wind_speed = instant.wind_speed.unwrap_or_default();

        WeatherMoment {
            temp,
            apparent: australian_apparent_temp(temp, humidity, wind_speed),
            humidity,
            weather: translated.clone(),
            weather_verbose: translated,
            wind: wind_speed,
            wind_kmh: wind_speed * 3.6,
            wind_direction: instant.wind_from_direction,
            icon: weather_to_icon(summary, is_night),
        }
    }

    fn to_aggregate(&self) -> ForecastAggregateSegment {
        let instant = &self.data.instant.details;

        let apparent = if let (Some(air_temperature), Some(relative_humidity), Some(wind_speed)) = (
            instant.air_temperature,
            instant.relative_humidity,
            instant.wind_speed,
        ) {
            Some(australian_apparent_temp(
                air_temperature,
                relative_humidity,
                wind_speed,
            ))
        } else {
            None
        };

        ForecastAggregateSegment {
            temp: instant.air_temperature,
            apparent,
            humidity: instant.relative_humidity,
            wind: instant.wind_speed,
            wind_kmh: instant.wind_speed.map(|t| t * 3.6),
            wind_direction: instant.wind_from_direction,
        }
    }
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

static LEGENDS: LazyLock<Option<LegendsStore>> =
    LazyLock::new(|| serde_json::from_str(include_str!("met_no_legends.json")).ok());

const FORECAST_URL: &str = "https://api.met.no/weatherapi/locationforecast/2.0/compact";

#[async_trait]
impl WeatherProvider for Service<'_> {
    async fn get_weather(
        &self,
        location: Option<&Coordinates>,
        need_forecast: bool,
    ) -> Result<WeatherResult> {
        let (lat, lon) = location
            .as_ref()
            .map(|loc| (loc.latitude.to_string(), loc.longitude.to_string()))
            .or_else(|| self.config.coordinates.clone())
            .error("No location given")?;

        let querystr: HashMap<&str, String> = map! {
            "lat" => lat,
            "lon" => lon,
            [if let Some(alt) = &self.config.altitude] "altitude" => alt,
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

        let forecast_hours = self.config.forecast_hours;
        let location_name = location.map_or("Unknown".to_string(), |c| c.city.clone());

        let current_weather = data.properties.timeseries.first().unwrap().to_moment(self);

        if !need_forecast || forecast_hours == 0 {
            return Ok(WeatherResult {
                location: location_name,
                current_weather,
                forecast: None,
            });
        }

        if data.properties.timeseries.len() < forecast_hours {
            return Err(Error::new(
                format!("Unable to fetch the specified number of forecast_hours specified {}, only {} hours available", forecast_hours, data.properties.timeseries.len()),
            ))?;
        }

        let data_agg: Vec<ForecastAggregateSegment> = data
            .properties
            .timeseries
            .iter()
            .take(forecast_hours)
            .map(|f| f.to_aggregate())
            .collect();

        let fin = data.properties.timeseries[forecast_hours - 1].to_moment(self);

        let forecast = Some(Forecast::new(&data_agg, fin));

        Ok(WeatherResult {
            location: location_name,
            current_weather,
            forecast,
        })
    }
}

fn weather_to_icon(weather: &str, is_night: bool) -> WeatherIcon {
    match weather {
        "cloudy" | "partlycloudy" | "fair" => WeatherIcon::Clouds{is_night},
        "fog" => WeatherIcon::Fog{is_night},
        "clearsky" => WeatherIcon::Clear{is_night},
        "heavyrain" | "heavyrainshowers" | "lightrain" | "lightrainshowers" | "rain"
        | "rainshowers" => WeatherIcon::Rain{is_night},
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
        | "lightssleetshowersandthunder" // There's a typo in the api it will be fixed in the next version to the following entry
        | "lightsleetshowersandthunder"
        | "lightssnowshowersandthunder"// There's a typo in the api it will be fixed in the next version to the following entry
        | "lightsnowshowersandthunder"
        | "lightrainshowersandthunder" => WeatherIcon::Thunder{is_night},
        "heavysleet" | "heavysleetshowers" | "heavysnow" | "heavysnowshowers" | "lightsleet"
        | "lightsleetshowers" | "lightsnow" | "lightsnowshowers" | "sleet" | "sleetshowers"
        | "snow" | "snowshowers" => WeatherIcon::Snow,
        _ => WeatherIcon::Default,
    }
}
