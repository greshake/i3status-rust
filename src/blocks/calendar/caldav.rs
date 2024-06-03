use std::{str::FromStr, time::Duration, vec};

use chrono::{DateTime, Local, Utc};
use icalendar::{Component, EventLike};
use reqwest::{
    self,
    header::{HeaderMap, HeaderValue, CONTENT_TYPE},
    ClientBuilder, Method, Url,
};
use serde::Deserialize;

use super::{
    auth::{Auth, Authorize},
    CalendarError,
};

#[derive(Clone, Debug)]
pub struct Event {
    pub uid: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub url: Option<String>,
    pub start_at: Option<DateTime<Utc>>,
    pub end_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Debug)]
pub struct Calendar {
    pub url: Url,
    pub name: String,
}

pub struct Client {
    url: Url,
    client: reqwest::Client,
    auth: Auth,
}

impl Client {
    pub fn new(url: Url, auth: Auth) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/xml"));
        Self {
            url,
            client: ClientBuilder::new()
                .timeout(Duration::from_secs(10))
                .default_headers(headers)
                .build()
                .expect("A valid http client"),
            auth,
        }
    }
    async fn propfind_request(
        &mut self,
        url: Url,
        depth: usize,
        body: String,
    ) -> Result<Multistatus, CalendarError> {
        let request = self
            .client
            .request(Method::from_str("PROPFIND").expect("A valid method"), url)
            .body(body.clone())
            .headers(self.auth.headers().await)
            .header("Depth", depth)
            .build()
            .expect("A valid propfind request");
        self.call(request).await
    }

    async fn report_request(
        &mut self,
        url: Url,
        depth: usize,
        body: String,
    ) -> Result<Multistatus, CalendarError> {
        let request = self
            .client
            .request(Method::from_str("REPORT").expect("A valid method"), url)
            .body(body)
            .headers(self.auth.headers().await)
            .header("Depth", depth)
            .build()
            .expect("A valid report request");
        self.call(request).await
    }

    async fn call(&mut self, request: reqwest::Request) -> Result<Multistatus, CalendarError> {
        let mut retries = 0;
        loop {
            let result = self
                .client
                .execute(request.try_clone().expect("Request to be cloneable"))
                .await?;
            match result.error_for_status() {
                Err(err) if retries == 0 => {
                    self.auth.handle_error(err).await?;
                    retries += 1;
                }
                Err(err) => return Err(CalendarError::Http(err)),
                Ok(result) => return Ok(quick_xml::de::from_str(result.text().await?.as_str())?),
            };
        }
    }

    async fn user_principal_url(&mut self) -> Result<Url, CalendarError> {
        let multi_status = self
            .propfind_request(self.url.clone(), 1, CURRENT_USER_PRINCIPAL.into())
            .await?;
        parse_href(multi_status, self.url.clone())
    }

    async fn home_set_url(&mut self, user_principal_url: Url) -> Result<Url, CalendarError> {
        let multi_status = self
            .propfind_request(user_principal_url, 0, CALENDAR_HOME_SET.into())
            .await?;
        parse_href(multi_status, self.url.clone())
    }

    async fn calendars_query(&mut self, home_set_url: Url) -> Result<Vec<Calendar>, CalendarError> {
        let multi_status = self
            .propfind_request(home_set_url, 1, CALENDAR_REQUEST.into())
            .await?;
        parse_calendars(multi_status, self.url.clone())
    }

    pub async fn calendars(&mut self) -> Result<Vec<Calendar>, CalendarError> {
        let user_principal_url = self.user_principal_url().await?;
        let home_set_url = self.home_set_url(user_principal_url).await?;
        self.calendars_query(home_set_url).await
    }

    pub async fn events(
        &mut self,
        calendar: &Calendar,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Event>, CalendarError> {
        let multi_status = self
            .report_request(calendar.url.clone(), 1, calendar_events_request(start, end))
            .await?;
        parse_events(multi_status)
    }

    pub async fn authorize(&mut self) -> Result<Authorize, CalendarError> {
        self.auth.authorize().await
    }

    pub async fn ask_user(&mut self, authorize: Authorize) -> Result<(), CalendarError> {
        match authorize {
            Authorize::Completed => Ok(()),
            Authorize::AskUser(authorize_url) => self.auth.ask_user(authorize_url).await,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename = "multistatus")]
struct Multistatus {
    #[serde(rename = "response", default)]
    responses: Vec<Response>,
}

#[derive(Debug, Deserialize)]
struct Response {
    href: String,
    #[serde(rename = "propstat", default)]
    propstats: Vec<Propstat>,
}

impl Response {
    fn valid_props(self) -> Vec<PropValue> {
        self.propstats
            .into_iter()
            .filter(|p| p.status.contains("200"))
            .flat_map(|p| p.prop.values.into_iter())
            .collect()
    }
}

#[derive(Debug, Deserialize)]
struct Propstat {
    status: String,
    prop: Prop,
}

#[derive(Debug, Deserialize)]
struct Prop {
    #[serde(rename = "$value")]
    pub values: Vec<PropValue>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum PropValue {
    CurrentUserPrincipal(HrefProperty),
    CalendarHomeSet(HrefProperty),
    SupportedCalendarComponentSet(SupportedCalendarComponentSet),
    #[serde(rename = "displayname")]
    DisplayName(String),
    #[serde(rename = "resourcetype")]
    ResourceType(ResourceTypes),
    CalendarData(String),
}

#[derive(Debug, Deserialize)]
pub struct HrefProperty {
    href: String,
}

#[derive(Debug, Deserialize)]
struct ResourceTypes {
    #[serde(rename = "$value")]
    pub values: Vec<ResourceType>,
}

impl ResourceTypes {
    fn is_calendar(&self) -> bool {
        self.values.contains(&ResourceType::Calendar)
    }
}
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum ResourceType {
    Calendar,
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Deserialize)]
struct SupportedCalendarComponentSet {
    comp: Option<Comp>,
}
impl SupportedCalendarComponentSet {
    fn supports_events(&self) -> bool {
        self.comp.as_ref().is_some_and(|v| v.name == "VEVENT")
    }
}

#[derive(Debug, Deserialize)]
struct Comp {
    #[serde(rename = "@name", default)]
    name: String,
}

fn parse_href(multi_status: Multistatus, base_url: Url) -> Result<Url, CalendarError> {
    let props = multi_status
        .responses
        .into_iter()
        .flat_map(|r| r.valid_props().into_iter())
        .next();
    match props.ok_or_else(|| CalendarError::Parsing("Property not found".into()))? {
        PropValue::CurrentUserPrincipal(href) | PropValue::CalendarHomeSet(href) => base_url
            .join(&href.href)
            .map_err(|e| CalendarError::Parsing(e.to_string())),
        _ => Err(CalendarError::Parsing("Invalid property".into())),
    }
}

fn parse_calendars(
    multi_status: Multistatus,
    base_url: Url,
) -> Result<Vec<Calendar>, CalendarError> {
    let mut result = vec![];
    for response in multi_status.responses {
        let mut is_calendar = false;
        let mut supports_events = false;
        let mut name = None;
        let href = response.href.clone();
        for prop in response.valid_props() {
            match prop {
                PropValue::SupportedCalendarComponentSet(comp) => {
                    supports_events = comp.supports_events();
                }
                PropValue::DisplayName(display_name) => name = Some(display_name),
                PropValue::ResourceType(ty) => is_calendar = ty.is_calendar(),
                _ => {}
            }
        }
        if is_calendar && supports_events {
            if let Some(name) = name {
                result.push(Calendar {
                    name,
                    url: base_url
                        .join(&href)
                        .map_err(|_| CalendarError::Parsing("Malformed calendar url".into()))?,
                });
            }
        }
    }
    Ok(result)
}

fn parse_events(multi_status: Multistatus) -> Result<Vec<Event>, CalendarError> {
    let mut result = vec![];
    for response in multi_status.responses {
        for prop in response.valid_props() {
            if let PropValue::CalendarData(data) = prop {
                let calendar =
                    icalendar::Calendar::from_str(&data).map_err(CalendarError::Parsing)?;
                for component in calendar.components {
                    if let icalendar::CalendarComponent::Event(event) = component {
                        let start_at = event.get_start().and_then(|d| match d {
                            icalendar::DatePerhapsTime::DateTime(dt) => dt.try_into_utc(),
                            icalendar::DatePerhapsTime::Date(d) => d
                                .and_hms_opt(0, 0, 0)
                                .and_then(|d| d.and_local_timezone(Local).earliest())
                                .map(|d| d.to_utc()),
                        });
                        let end_at = event.get_end().and_then(|d| match d {
                            icalendar::DatePerhapsTime::DateTime(dt) => dt.try_into_utc(),
                            icalendar::DatePerhapsTime::Date(d) => d
                                .and_hms_opt(23, 59, 59)
                                .and_then(|d| d.and_local_timezone(Local).earliest())
                                .map(|d| d.to_utc()),
                        });
                        result.push(Event {
                            uid: event.get_uid().map(Into::into),
                            summary: event.get_summary().map(Into::into),
                            description: event.get_description().map(Into::into),
                            location: event.get_location().map(Into::into),
                            url: event.get_url().map(Into::into),
                            start_at,
                            end_at,
                        });
                    }
                }
            }
        }
    }
    Ok(result)
}

static CURRENT_USER_PRINCIPAL: &str = r#"<d:propfind xmlns:d="DAV:">
          <d:prop>
            <d:current-user-principal />
          </d:prop>
        </d:propfind>"#;

static CALENDAR_HOME_SET: &str = r#"<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" >
            <d:prop>
                <c:calendar-home-set />
            </d:prop>
        </d:propfind>"#;

static CALENDAR_REQUEST: &str = r#"<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" >
            <d:prop>
                <d:displayname />
                <d:resourcetype />
                <c:supported-calendar-component-set />
            </d:prop>
        </d:propfind>"#;

pub fn calendar_events_request(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    const DATE_FORMAT: &str = "%Y%m%dT%H%M%SZ";
    let start = start.format(DATE_FORMAT);
    let end = end.format(DATE_FORMAT);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
        <c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
        <d:prop>
            <c:calendar-data/>
        </d:prop>
        <c:filter>
            <c:comp-filter name="VCALENDAR">
                <c:comp-filter name="VEVENT">
                    <c:time-range start="{start}" end="{end}" />
                </c:comp-filter>
            </c:comp-filter>
        </c:filter>
        </c:calendar-query>"#
    )
}
