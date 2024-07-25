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

    fn get_weather_instant(&self, forecast_data: &ForecastData) -> WeatherMoment {
        let instant = &forecast_data.instant.details;

        let mut symbol_code_split = forecast_data
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

        let translated = translate(self.legend, summary, &self.config.lang);

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

static LEGENDS: LazyLock<Option<LegendsStore>> =
    LazyLock::new(|| serde_json::from_str(include_str!("met_no_legends.json")).ok());

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

        let forecast = if !need_forecast || forecast_hours == 0 {
            None
        } else {
            let mut temp_avg = 0.0;
            let mut temp_min = f64::MAX;
            let mut temp_max = f64::MIN;
            let mut temp_count = 0.0;
            let mut humidity_avg = 0.0;
            let mut humidity_min = f64::MAX;
            let mut humidity_max = f64::MIN;
            let mut humidity_count = 0.0;
            let mut wind_forecasts = Vec::new();
            let mut apparent_avg = 0.0;
            let mut apparent_min = f64::MAX;
            let mut apparent_max = f64::MIN;
            let mut apparent_count = 0.0;
            if data.properties.timeseries.len() < forecast_hours {
                Err(Error::new(
                format!("Unable to fetch the specified number of forecast_hours specified {}, only {} hours available", forecast_hours, data.properties.timeseries.len()),
            ))?;
            }
            for forecast_time_step in data.properties.timeseries.iter().take(forecast_hours) {
                let forecast_instant = &forecast_time_step.data.instant.details;
                if let Some(air_temperature) = forecast_instant.air_temperature {
                    temp_avg += air_temperature;
                    temp_min = temp_min.min(air_temperature);
                    temp_max = temp_max.max(air_temperature);
                    temp_count += 1.0;
                }
                if let Some(relative_humidity) = forecast_instant.relative_humidity {
                    humidity_avg += relative_humidity;
                    humidity_min = humidity_min.min(relative_humidity);
                    humidity_max = humidity_max.max(relative_humidity);
                    humidity_count += 1.0;
                }
                if let Some(wind_speed) = forecast_instant.wind_speed {
                    wind_forecasts.push(Wind {
                        speed: wind_speed,
                        degrees: forecast_instant.wind_from_direction,
                    });
                }
                if let (Some(air_temperature), Some(relative_humidity), Some(wind_speed)) = (
                    forecast_instant.air_temperature,
                    forecast_instant.relative_humidity,
                    forecast_instant.wind_speed,
                ) {
                    let apparent =
                        australian_apparent_temp(air_temperature, relative_humidity, wind_speed);
                    apparent_avg += apparent;
                    apparent_min = apparent_min.min(apparent);
                    apparent_max = apparent_max.max(apparent);
                    apparent_count += 1.0;
                }
            }
            temp_avg /= temp_count;
            humidity_avg /= humidity_count;
            apparent_avg /= apparent_count;
            let Wind {
                speed: wind_avg,
                degrees: direction_avg,
            } = average_wind(&wind_forecasts);
            let Wind {
                speed: wind_min,
                degrees: direction_min,
            } = wind_forecasts
                .iter()
                .min_by(|x, y| x.speed.total_cmp(&y.speed))
                .error("No min wind")?;
            let Wind {
                speed: wind_max,
                degrees: direction_max,
            } = wind_forecasts
                .iter()
                .min_by(|x, y| x.speed.total_cmp(&y.speed))
                .error("No max wind")?;

            Some(Forecast {
                avg: ForecastAggregate {
                    temp: temp_avg,
                    apparent: apparent_avg,
                    humidity: humidity_avg,
                    wind: wind_avg,
                    wind_kmh: wind_avg * 3.6,
                    wind_direction: direction_avg,
                },
                min: ForecastAggregate {
                    temp: temp_min,
                    apparent: apparent_min,
                    humidity: humidity_min,
                    wind: *wind_min,
                    wind_kmh: wind_min * 3.6,
                    wind_direction: *direction_min,
                },
                max: ForecastAggregate {
                    temp: temp_max,
                    apparent: apparent_max,
                    humidity: humidity_max,
                    wind: *wind_max,
                    wind_kmh: wind_max * 3.6,
                    wind_direction: *direction_max,
                },
                fin: self.get_weather_instant(&data.properties.timeseries[forecast_hours - 1].data),
            })
        };

        Ok(WeatherResult {
            location: location.map_or("Unknown".to_string(), |c| c.city.clone()),
            current_weather: self
                .get_weather_instant(&data.properties.timeseries.first().unwrap().data),
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
