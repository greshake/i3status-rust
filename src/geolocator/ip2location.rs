use super::*;

const IP_API_URL: &str = "https://api.ip2location.io/";
pub(super) const API_KEY_ENV: &str = "IP2LOCATION_API_KEY";

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "getenv_api_key")]
    pub api_key: Option<String>,
}

fn getenv_api_key() -> Option<String> {
    std::env::var(API_KEY_ENV).ok()
}

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(flatten)]
    data: Option<Ip2LocationAddressInfo>,
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct Ip2LocationAddressInfo {
    // Provided without api key
    ip: String,
    country_code: String,
    country_name: String,
    region_name: String,
    city_name: String,
    latitude: f64,
    longitude: f64,
    zip_code: String,
    time_zone: String,
    asn: String,
    // #[serde(rename = "as")]
    // as_: String,
    // is_proxy: bool,

    // Requires api key
    // isp
    // domain
    // net_speed
    // idd_code
    // area_code
    // weather_station_code
    // weather_station_name
    // mcc
    // mnc
    // mobile_brand
    // elevation
    // usage_type
    // address_type
    // ads_category
    // ads_category_name
    // district
    continent: Option<Continent>,
    country: Option<Country>,
    region: Option<Region>,
    // city.name
    // city.translation
    time_zone_info: Option<TimeZoneInfo>,
    // geotargeting.metro
    // fraud_score
    // proxy.last_seen
    // proxy.proxy_type
    // proxy.threat
    // proxy.provider
    // proxy.is_vpn
    // proxy.is_tor
    // proxy.is_data_center
    // proxy.is_public_proxy
    // proxy.is_web_proxy
    // proxy.is_web_crawler
    // proxy.is_residential_proxy
    // proxy.is_spammer
    // proxy.is_scanner
    // proxy.is_botnet
    // proxy.is_consumer_privacy_network
    // proxy.is_enterprise_private_network
}

#[derive(Deserialize)]
struct Continent {
    // name,
    code: String,
    // hemisphere,
    // translation,
}

#[derive(Deserialize)]
struct Country {
    // name: String,
    alpha3_code: String,
    // numeric_code: String,
    // demonym: String,
    // flag: String,
    capital: String,
    total_area: f64,
    population: f64,
    currency: Currency,
    language: Language,
    tld: String,
    // translation,
}

#[derive(Deserialize)]
struct Currency {
    name: String,
    code: String,
    // translation,
}
#[derive(Deserialize)]
struct Language {
    // name: String,
    code: String,
}
#[derive(Deserialize)]
struct Region {
    // name: String,
    code: String,
    // translation,
}

#[derive(Deserialize)]
struct TimeZoneInfo {
    olson: String,
    // current_time: String
    // gmt_offset: String
    // is_dst: String
    // sunrise: String
    // sunset: String
}

impl From<Ip2LocationAddressInfo> for IPAddressInfo {
    fn from(val: Ip2LocationAddressInfo) -> Self {
        let mut info = IPAddressInfo {
            ip: val.ip,
            city: val.city_name,
            latitude: val.latitude,
            longitude: val.longitude,
            region: Some(val.region_name),
            country: Some(val.country_code.clone()),
            country_name: Some(val.country_name),
            country_code: Some(val.country_code),
            postal: Some(val.zip_code),
            utc_offset: Some(val.time_zone),
            asn: Some(val.asn),
            ..Default::default()
        };

        if let Some(region) = val.region {
            info.region_code = Some(region.code);
        }

        if let Some(country) = val.country {
            info.country_area = Some(country.total_area);
            info.country_population = Some(country.population);
            info.currency = Some(country.currency.code);
            info.currency_name = Some(country.currency.name);
            info.languages = Some(country.language.code);
            info.country_tld = Some(country.tld);
            info.country_capital = Some(country.capital);
            info.country_code_iso3 = Some(country.alpha3_code);
        }

        if let Some(continent) = val.continent {
            info.continent_code = Some(continent.code);
        }

        if let Some(time_zone_info) = val.time_zone_info {
            info.timezone = Some(time_zone_info.olson);
        }

        info
    }
}

#[derive(Deserialize, Default, Debug)]
struct ApiError {
    error_code: u32,
    error_message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            "Error {}: {}",
            self.error_code, self.error_message
        ))
    }
}
impl StdError for ApiError {}

pub struct Ip2Location;

impl Ip2Location {
    pub fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("ip2location.io")
    }

    pub async fn get_info(
        &self,
        client: &reqwest::Client,
        api_key: &Option<String>,
    ) -> Result<IPAddressInfo> {
        let mut request_builder = client.get(IP_API_URL);

        if let Some(api_key) = api_key {
            request_builder = request_builder.query(&[("key", api_key)]);
        }

        let response: ApiResponse = request_builder
            .send()
            .await
            .error("Failed during request for current location")?
            .json()
            .await
            .error("Failed while parsing location API result")?;

        if let Some(error) = response.error {
            Err(Error {
                message: Some("ip2location.io error".into()),
                cause: Some(Arc::new(error)),
            })
        } else {
            Ok(response
                .data
                .error("Failed while parsing location API result")?
                .into())
        }
    }
}
