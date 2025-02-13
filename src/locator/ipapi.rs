use super::*;

const IP_API_URL: &str = "https://ipapi.co/json";

// This config is here just to make sure that no other config is provided
#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {}

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(flatten)]
    data: Option<IpApiAddressInfo>,
    #[serde(default)]
    error: bool,
    #[serde(default)]
    reason: ApiError,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct IpApiAddressInfo {
    ip: String,
    version: String,
    city: String,
    region: String,
    region_code: String,
    country: String,
    country_name: String,
    country_code: String,
    country_code_iso3: String,
    country_capital: String,
    country_tld: String,
    continent_code: String,
    in_eu: bool,
    postal: Option<String>,
    latitude: f64,
    longitude: f64,
    timezone: String,
    utc_offset: String,
    country_calling_code: String,
    currency: String,
    currency_name: String,
    languages: String,
    country_area: f64,
    country_population: f64,
    asn: String,
    org: String,
}

impl From<IpApiAddressInfo> for IPAddressInfo {
    fn from(val: IpApiAddressInfo) -> Self {
        IPAddressInfo {
            ip: val.ip,
            version: Some(val.version),
            city: val.city,
            region: Some(val.region),
            region_code: Some(val.region_code),
            country: Some(val.country),
            country_name: Some(val.country_name),
            country_code: Some(val.country_code),
            country_code_iso3: Some(val.country_code_iso3),
            country_capital: Some(val.country_capital),
            country_tld: Some(val.country_tld),
            continent_code: Some(val.continent_code),
            in_eu: Some(val.in_eu),
            postal: val.postal,
            latitude: val.latitude,
            longitude: val.longitude,
            timezone: Some(val.timezone),
            utc_offset: Some(val.utc_offset),
            country_calling_code: Some(val.country_calling_code),
            currency: Some(val.currency),
            currency_name: Some(val.currency_name),
            languages: Some(val.languages),
            country_area: Some(val.country_area),
            country_population: Some(val.country_population),
            asn: Some(val.asn),
            org: Some(val.org),
        }
    }
}

#[derive(Deserialize, Default, Debug)]
#[serde(transparent)]
struct ApiError(Option<String>);

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_deref().unwrap_or("Unknown Error"))
    }
}
impl StdError for ApiError {}

pub struct Ipapi;

impl Ipapi {
    pub fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("ipapi.co")
    }

    pub async fn get_info(&self, client: &reqwest::Client) -> Result<IPAddressInfo> {
        let response: ApiResponse = client
            .get(IP_API_URL)
            .send()
            .await
            .error("Failed during request for current location")?
            .json()
            .await
            .error("Failed while parsing location API result")?;

        if response.error {
            Err(Error {
                message: Some("ipapi.co error".into()),
                cause: Some(Arc::new(response.reason)),
            })
        } else {
            Ok(response
                .data
                .error("Failed while parsing location API result")?
                .into())
        }
    }
}
