use std::{
    cell::OnceCell,
    collections::{HashMap, HashSet},
    str::FromStr as _,
};

use base64::{Engine as _, prelude::BASE64_STANDARD_NO_PAD};
use chrono::{DateTime, Days, Duration, Local, TimeZone as _, Utc};
use icalendar::{
    CalendarDateTime, Component as _, DatePerhapsTime, EventLike as _, EventStatus,
    Tz as RecurrenceTz,
};
use percent_encoding::percent_decode_str;
use reqwest::Url;

use super::CalendarError;

const MAX_RECURRENCE_RESULTS: u16 = 4096;
const MAX_RECURRENCE_INTERVALS: i64 = 100_000;

#[derive(Clone, Debug)]
pub struct Event {
    pub uid: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub url: Option<String>,
    pub details_url: Option<Url>,
    pub is_recurring: bool,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
}

/// Derive undocumented Google Calendar links for users who explicitly opt into the heuristic.
///
/// Google does not guarantee that an iCalendar UID maps to its opaque event ID. Recurring events
/// need an occurrence-specific ID that is not available from a UID alone, so they are never given
/// a derived link here.
pub fn derive_google_event_details_urls(events: &mut [Event], source_url: &Url) {
    let Some(calendar_id) = google_calendar_id(source_url) else {
        return;
    };

    for event in events {
        if event.details_url.is_none() && !event.is_recurring {
            event.details_url = best_effort_google_event_details_url(&event.uid, &calendar_id);
        }
    }
}

fn google_calendar_id(source_url: &Url) -> Option<String> {
    let host = source_url.host_str()?;
    let mut segments: Vec<_> = source_url.path_segments()?.collect();
    if segments.last() == Some(&"") {
        segments.pop();
    }
    let encoded_calendar_id = match host {
        "calendar.google.com" => match segments.as_slice() {
            ["calendar", "ical", calendar_id, access, "basic.ics"]
                if *access == "public" || access.starts_with("private-") =>
            {
                calendar_id
            }
            _ => return None,
        },
        "apidata.googleusercontent.com" => match segments.as_slice() {
            ["caldav", "v2", calendar_id, "events"] => calendar_id,
            _ => return None,
        },
        _ => return None,
    };

    let calendar_id = percent_decode_str(encoded_calendar_id)
        .decode_utf8()
        .ok()?
        .into_owned();
    (!calendar_id.is_empty() && !calendar_id.chars().any(char::is_control))
        .then_some(calendar_id)
}

fn best_effort_google_event_details_url(uid: &str, calendar_id: &str) -> Option<Url> {
    let event_id = uid.strip_suffix("@google.com")?;
    if event_id.is_empty()
        || !event_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return None;
    }

    let eid = BASE64_STANDARD_NO_PAD.encode(format!("{event_id} {calendar_id}"));
    let mut url = Url::parse("https://calendar.google.com/calendar/event").ok()?;
    url.query_pairs_mut().append_pair("eid", &eid);
    Some(url)
}

pub fn parse_events(
    data: &str,
    event_search_start: DateTime<Utc>,
    event_search_end: DateTime<Utc>,
    source_url: Option<&Url>,
) -> Result<Vec<Event>, CalendarError> {
    let calendar_id = source_url.and_then(google_calendar_id);
    let calendar = icalendar::Calendar::from_str(data).map_err(CalendarError::Parsing)?;
    let events = || {
        calendar
            .components
            .iter()
            .filter_map(|component| component.as_event())
    };

    // A modified or cancelled recurrence is represented as another VEVENT with the same UID and
    // a RECURRENCE-ID. Suppress that instant from the master event; a non-cancelled override is
    // added below at its replacement DTSTART. RANGE=THISANDFUTURE changes every later instance;
    // the recurrence library does not implement that mode, so reject the affected series.
    let mut unsupported_series = HashSet::new();
    let mut overridden_recurrences = HashMap::<&str, HashSet<DateTime<Utc>>>::new();
    let mut invalid_overrides = HashSet::new();
    for (index, event) in events().enumerate() {
        let Some(uid) = event_uid(event) else {
            continue;
        };
        if recurrence_range(event).is_some_and(|range| range.eq_ignore_ascii_case("THISANDFUTURE"))
        {
            if unsupported_series.insert(uid) {
                log::warn!(
                    "Ignoring calendar event series with UID {uid:?}: \
                     RECURRENCE-ID;RANGE=THISANDFUTURE is not supported"
                );
            }
            continue;
        }
        if !event.properties().contains_key("RECURRENCE-ID") {
            continue;
        }
        let recurrence_id = event
            .get_recurrence_id()
            .ok_or_else(|| "RECURRENCE-ID is invalid".to_owned())
            .and_then(|value| date_to_utc(&value, "RECURRENCE-ID").map(|date| date.utc));
        match recurrence_id {
            Ok(recurrence_id) => {
                overridden_recurrences
                    .entry(uid)
                    .or_default()
                    .insert(recurrence_id);
            }
            Err(error) => {
                warn_event(uid, &error);
                invalid_overrides.insert(index);
            }
        }
    }

    let mut result = vec![];
    for (index, event) in events().enumerate() {
        let Some(uid) = event_uid(event) else {
            log::warn!("Ignoring calendar event: UID is missing or empty");
            continue;
        };
        if unsupported_series.contains(uid) || invalid_overrides.contains(&index) {
            continue;
        }
        if event.get_status() == Some(EventStatus::Cancelled) {
            continue;
        }
        let declined = calendar_id.as_deref().is_some_and(|calendar_id| {
            event
                .multi_properties()
                .get("ATTENDEE")
                .is_some_and(|attendees| {
                    attendees.iter().any(|attendee| {
                        attendee
                            .params()
                            .get("PARTSTAT")
                            .is_some_and(|status| status.value().eq_ignore_ascii_case("DECLINED"))
                            && attendee.value().split_once(':').is_some_and(
                                |(scheme, address)| {
                                    scheme.eq_ignore_ascii_case("mailto")
                                        && address.eq_ignore_ascii_case(calendar_id)
                                },
                            )
                    })
                })
        });
        if declined {
            continue;
        }

        let (event_start, event_end_spec) = match event_bounds(event) {
            Ok(bounds) => bounds,
            Err(error) => {
                warn_event(uid, &error);
                continue;
            }
        };
        let event_end = match event_end_spec.end_at(event_start) {
            Ok(end) => end,
            Err(error) => {
                warn_event(uid, &error);
                continue;
            }
        };
        let properties = event.properties();
        let multi_properties = event.multi_properties();
        let is_override = properties.contains_key("RECURRENCE-ID");
        let is_recurring = is_override
            || properties.contains_key("RRULE")
            || multi_properties.contains_key("RDATE")
            || multi_properties.contains_key("EXDATE");
        let details_url = OnceCell::new();
        let mut add_if_visible = |start, end| {
            if overlaps_search_window(start, end, event_search_start, event_search_end) {
                result.push(event_at(
                    uid,
                    event,
                    start,
                    end,
                    is_recurring,
                    &details_url,
                ));
            }
        };

        if is_override {
            add_if_visible(event_start, event_end);
            continue;
        }

        if !is_recurring {
            add_if_visible(event_start, event_end);
            continue;
        }

        let search_duration = match event_end_spec.search_duration() {
            Ok(duration) => duration,
            Err(error) => {
                warn_event(uid, &error);
                continue;
            }
        };
        let recurrence_search_start = event_search_start
            .checked_sub_signed(search_duration)
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        let recurrence = match event.get_recurrence() {
            Ok(recurrence) => recurrence,
            Err(error) => {
                warn_event(uid, &format!("invalid recurrence: {error}"));
                add_if_visible(event_start, event_end);
                continue;
            }
        };
        if recurrence_requires_too_much_work(&recurrence, event_search_end) {
            warn_event(
                uid,
                "recurrence expansion would require too many frequency intervals",
            );
            add_if_visible(event_start, event_end);
            continue;
        }
        let recurrence_result = recurrence
            .after(recurrence_search_start.with_timezone(&RecurrenceTz::UTC))
            .before(event_search_end.with_timezone(&RecurrenceTz::UTC))
            .all(MAX_RECURRENCE_RESULTS);
        if recurrence_result.limited {
            warn_event(
                uid,
                &format!(
                    "recurrence expansion reached the {MAX_RECURRENCE_RESULTS}-instance safety limit"
                ),
            );
        }

        let overridden = overridden_recurrences.get(uid);
        for start in recurrence_result
            .dates
            .into_iter()
            .map(|start| start.to_utc())
            .filter(|start| !overridden.is_some_and(|dates| dates.contains(start)))
        {
            let end = match event_end_spec.end_at(start) {
                Ok(end) => end,
                Err(error) => {
                    warn_event(uid, &error);
                    continue;
                }
            };
            add_if_visible(start, end);
        }
    }
    Ok(result)
}

fn event_uid(event: &icalendar::Event) -> Option<&str> {
    event.get_uid().filter(|uid| !uid.is_empty())
}

fn recurrence_range(event: &icalendar::Event) -> Option<&str> {
    event
        .properties()
        .get("RECURRENCE-ID")?
        .params()
        .get("RANGE")
        .map(|parameter| parameter.value())
}

fn warn_event(uid: &str, error: &str) {
    log::warn!("Ignoring calendar event with UID {uid:?}: {error}");
}

fn event_bounds(event: &icalendar::Event) -> Result<(DateTime<Utc>, EventEnd), String> {
    let start_value = event
        .get_start()
        .ok_or_else(|| "DTSTART is missing or invalid".to_owned())?;
    let is_all_day = matches!(start_value, DatePerhapsTime::Date(_));
    let start = date_to_utc(&start_value, "DTSTART")?;

    let has_end = event.properties().contains_key("DTEND");
    let duration = event.property_value("DURATION");
    if has_end && duration.is_some() {
        return Err("DTEND and DURATION must not both be present".into());
    }

    let end = if has_end {
        let end_value = event
            .get_end()
            .ok_or_else(|| "DTEND is invalid".to_owned())?;
        validate_dtend_form(&start_value, &end_value)?;
        let end = date_to_utc(&end_value, "DTEND")?.utc;
        if end <= start.utc {
            return Err("DTEND must be later than DTSTART".into());
        }
        EventEnd::Exact(end - start.utc)
    } else if let Some(value) = duration {
        let duration = parse_duration(value)?;
        if duration.days == 0 && duration.clock == Duration::zero() {
            return Err("DURATION must be positive".into());
        }
        if is_all_day && duration.clock != Duration::zero() {
            return Err("an all-day event DURATION must contain only days or weeks".into());
        }
        EventEnd::Nominal {
            duration,
            timezone: start.timezone,
        }
    } else if is_all_day {
        EventEnd::Nominal {
            duration: IcalDuration {
                days: 1,
                clock: Duration::zero(),
            },
            timezone: start.timezone,
        }
    } else {
        // RFC 5545 defines an event without DTEND or DURATION as ending at DTSTART.
        EventEnd::Exact(Duration::zero())
    };
    Ok((start.utc, end))
}

fn validate_dtend_form(start: &DatePerhapsTime, end: &DatePerhapsTime) -> Result<(), String> {
    match (start, end) {
        (DatePerhapsTime::Date(_), DatePerhapsTime::Date(_)) => Ok(()),
        (DatePerhapsTime::DateTime(start), DatePerhapsTime::DateTime(end))
            if matches!(start, CalendarDateTime::Floating(_))
                == matches!(end, CalendarDateTime::Floating(_)) =>
        {
            Ok(())
        }
        (DatePerhapsTime::DateTime(_), DatePerhapsTime::DateTime(_)) => Err(
            "DTEND must use local floating time if and only if DTSTART uses local floating time"
                .into(),
        ),
        _ => Err("DTEND must have the same value type as DTSTART".into()),
    }
}

#[derive(Clone, Copy, Debug)]
struct IcalDuration {
    days: u64,
    clock: Duration,
}

fn parse_duration(value: &str) -> Result<IcalDuration, String> {
    let bytes = value.as_bytes();
    let mut index = usize::from(bytes.first() == Some(&b'+'));
    if bytes.first() == Some(&b'-') {
        return Err("DURATION must not be negative".into());
    }
    if bytes.get(index) != Some(&b'P') {
        return Err("DURATION must start with `P` (after an optional `+`)".into());
    }
    index += 1;
    if index == bytes.len() {
        return Err("DURATION has no components".into());
    }

    let mut days = 0;
    let mut clock_seconds = 0_u64;

    if bytes.get(index).is_some_and(u8::is_ascii_digit) {
        let number = parse_duration_number(bytes, &mut index)?;
        match bytes.get(index) {
            Some(b'W') => {
                index += 1;
                if index != bytes.len() {
                    return Err("a week DURATION cannot contain other components".into());
                }
                days = number
                    .checked_mul(7)
                    .ok_or_else(|| "DURATION is too large".to_owned())?;
            }
            Some(b'D') => {
                index += 1;
                days = number;
            }
            _ => return Err("invalid DURATION date component".into()),
        }
    }

    if index < bytes.len() {
        if bytes[index] != b'T' {
            return Err("invalid DURATION component".into());
        }
        index += 1;
        if index == bytes.len() {
            return Err("DURATION `T` must be followed by a time component".into());
        }
        let mut previous_rank = 0;
        while index < bytes.len() {
            let number = parse_duration_number(bytes, &mut index)?;
            let (rank, multiplier) = match bytes.get(index) {
                Some(b'H') => (1, 60 * 60),
                Some(b'M') => (2, 60),
                Some(b'S') => (3, 1),
                _ => return Err("invalid DURATION time component".into()),
            };
            if previous_rank != 0 && rank != previous_rank + 1 {
                return Err(
                    "DURATION time components are repeated, out of order, or skip a component"
                        .into(),
                );
            }
            index += 1;
            previous_rank = rank;
            clock_seconds = clock_seconds
                .checked_add(
                    number
                        .checked_mul(multiplier)
                        .ok_or_else(|| "DURATION is too large".to_owned())?,
                )
                .ok_or_else(|| "DURATION is too large".to_owned())?;
        }
    }
    let clock_seconds =
        i64::try_from(clock_seconds).map_err(|_| "DURATION is too large".to_owned())?;
    Ok(IcalDuration {
        days,
        clock: Duration::try_seconds(clock_seconds)
            .ok_or_else(|| "DURATION is too large".to_owned())?,
    })
}

fn parse_duration_number(bytes: &[u8], index: &mut usize) -> Result<u64, String> {
    let start = *index;
    while bytes.get(*index).is_some_and(u8::is_ascii_digit) {
        *index += 1;
    }
    if start == *index {
        return Err("DURATION component is missing a number".into());
    }
    std::str::from_utf8(&bytes[start..*index])
        .expect("ASCII digits are valid UTF-8")
        .parse()
        .map_err(|_| "DURATION is too large".into())
}

#[derive(Clone, Copy, Debug)]
enum CalendarTimezone {
    Utc,
    Local,
    Named(chrono_tz::Tz),
}

#[derive(Clone, Copy, Debug)]
struct ConvertedDate {
    utc: DateTime<Utc>,
    timezone: CalendarTimezone,
}

fn date_to_utc(value: &DatePerhapsTime, property: &str) -> Result<ConvertedDate, String> {
    match value {
        DatePerhapsTime::DateTime(CalendarDateTime::Floating(value)) => {
            let value = Local.from_local_datetime(value).single().ok_or_else(|| {
                format!("{property} is ambiguous or invalid in the local time zone")
            })?;
            Ok(ConvertedDate {
                utc: value.to_utc(),
                timezone: CalendarTimezone::Local,
            })
        }
        DatePerhapsTime::DateTime(CalendarDateTime::Utc(value)) => Ok(ConvertedDate {
            utc: *value,
            timezone: CalendarTimezone::Utc,
        }),
        DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone { date_time, tzid }) => {
            let timezone = tzid.parse::<chrono_tz::Tz>().map_err(|_| {
                format!(
                    "{property} uses unknown or unsupported TZID {tzid:?}; \
                     embedded VTIMEZONE definitions are not supported"
                )
            })?;
            let value = timezone
                .from_local_datetime(date_time)
                .single()
                .ok_or_else(|| {
                    format!("{property} is ambiguous or invalid in time zone {tzid:?}")
                })?;
            Ok(ConvertedDate {
                utc: value.to_utc(),
                timezone: CalendarTimezone::Named(timezone),
            })
        }
        DatePerhapsTime::Date(value) => {
            let value = (*value)
                .and_hms_opt(0, 0, 0)
                .and_then(|value| Local.from_local_datetime(&value).single())
                .ok_or_else(|| format!("{property} is invalid in the local time zone"))?;
            Ok(ConvertedDate {
                utc: value.to_utc(),
                timezone: CalendarTimezone::Local,
            })
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum EventEnd {
    Exact(Duration),
    Nominal {
        duration: IcalDuration,
        timezone: CalendarTimezone,
    },
}

impl EventEnd {
    fn end_at(self, start: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
        match self {
            Self::Exact(duration) => start
                .checked_add_signed(duration)
                .ok_or_else(|| "event end is outside the supported date range".to_owned()),
            Self::Nominal { duration, timezone } => {
                let days = Days::new(duration.days);
                let after_days = match timezone {
                    CalendarTimezone::Utc => start.checked_add_days(days),
                    CalendarTimezone::Local => start
                        .with_timezone(&Local)
                        .checked_add_days(days)
                        .map(|date| date.to_utc()),
                    CalendarTimezone::Named(timezone) => start
                        .with_timezone(&timezone)
                        .checked_add_days(days)
                        .map(|date| date.to_utc()),
                }
                .ok_or_else(|| {
                    "DURATION ends at an ambiguous, invalid, or out-of-range local time".to_owned()
                })?;
                after_days
                    .checked_add_signed(duration.clock)
                    .ok_or_else(|| "event end is outside the supported date range".to_owned())
            }
        }
    }

    fn search_duration(self) -> Result<Duration, String> {
        match self {
            Self::Exact(duration) => Ok(duration),
            Self::Nominal { duration, .. } => {
                // A nominal day can cross a time-zone transition. Twenty-six hours is deliberately
                // conservative for deciding how far before the display window expansion must begin.
                let hours = duration
                    .days
                    .checked_mul(26)
                    .and_then(|hours| i64::try_from(hours).ok())
                    .ok_or_else(|| "DURATION is too large".to_owned())?;
                Duration::try_hours(hours)
                    .and_then(|days| days.checked_add(&duration.clock))
                    .ok_or_else(|| "DURATION is too large".to_owned())
            }
        }
    }
}

fn recurrence_requires_too_much_work(
    recurrence: &icalendar::RRuleSet,
    expansion_end: DateTime<Utc>,
) -> bool {
    let recurrence_start = recurrence.get_dt_start().to_utc();
    recurrence.get_rrule().iter().any(|rule| {
        let target = rule
            .get_until()
            .map(|until| until.to_utc().min(expansion_end))
            .unwrap_or(expansion_end);
        let elapsed = target.signed_duration_since(recurrence_start).num_seconds();
        if elapsed <= 0 {
            return false;
        }
        let period_seconds = match rule.get_freq() {
            icalendar::Frequency::Secondly => 1,
            icalendar::Frequency::Minutely => 60,
            icalendar::Frequency::Hourly => 60 * 60,
            icalendar::Frequency::Daily => 23 * 60 * 60,
            icalendar::Frequency::Weekly => 6 * 24 * 60 * 60,
            icalendar::Frequency::Monthly => 27 * 24 * 60 * 60,
            icalendar::Frequency::Yearly => 365 * 24 * 60 * 60,
        };
        let interval = i64::from(rule.get_interval().max(1));
        elapsed / period_seconds / interval > MAX_RECURRENCE_INTERVALS
    })
}

fn overlaps_search_window(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    search_start: DateTime<Utc>,
    search_end: DateTime<Utc>,
) -> bool {
    end > search_start && start < search_end
}

fn event_at(
    uid: &str,
    event: &icalendar::Event,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    is_recurring: bool,
    details_url: &OnceCell<Option<Url>>,
) -> Event {
    let url = event.get_url().map(Into::into);
    let description: Option<String> = event.get_description().map(Into::into);
    let details_url = details_url
        .get_or_init(|| {
            url.as_deref()
                .and_then(|url| Url::parse(url).ok())
                .or_else(|| {
                    event
                        .property_value("X-GOOGLE-CONFERENCE")
                        .and_then(|url| Url::parse(url).ok())
                        .filter(|url| {
                            url.scheme() == "https" && url.host_str() == Some("meet.google.com")
                        })
                })
                .or_else(|| {
                    description
                        .as_deref()?
                        .split_whitespace()
                        .map(|candidate| {
                            candidate.trim_matches(|character: char| {
                                matches!(
                                    character,
                                    '<' | '>' | '(' | ')' | '[' | ']' | '"' | '\''
                                )
                            })
                        })
                        .filter(|candidate| candidate.starts_with("https://"))
                        .find_map(|candidate| {
                            let url = Url::parse(candidate).ok()?;
                            let host = url.host_str()?;
                            let meeting_id = url.path().strip_prefix("/j/")?;
                            ((host == "zoom.us" || host.ends_with(".zoom.us"))
                                && !meeting_id.is_empty()
                                && meeting_id
                                    .chars()
                                    .all(|character| character.is_ascii_digit()))
                            .then_some(url)
                        })
                    })
                })
        .clone();
    Event {
        uid: uid.into(),
        summary: event.get_summary().map(Into::into),
        description,
        location: event.get_location().map(Into::into),
        url,
        details_url,
        is_recurring,
        start_at,
        end_at,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};

    use super::*;

    macro_rules! calendar {
        ($($line:literal),* $(,)?) => {
            concat!(
                "BEGIN:VCALENDAR\r\n",
                "VERSION:2.0\r\n",
                $($line,)*
                "END:VCALENDAR\r\n",
            )
        };
    }

    macro_rules! event {
        ($($line:literal),* $(,)?) => {
            calendar!("BEGIN:VEVENT\r\n", $($line,)* "END:VEVENT\r\n")
        };
    }

    fn utc(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
            .unwrap()
    }

    fn first_event(data: &str) -> icalendar::Event {
        icalendar::Calendar::from_str(data)
            .unwrap()
            .components
            .into_iter()
            .find_map(|component| match component {
                icalendar::CalendarComponent::Event(event) => Some(event),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn parses_timed_event_and_metadata() {
        let data = event!(
            "UID:meeting\r\n",
            "DTSTART;TZID=America/Los_Angeles:20260729T090000\r\n",
            "DTEND;TZID=America/Los_Angeles:20260729T100000\r\n",
            "SUMMARY:Planning\r\n",
            "DESCRIPTION:Roadmap review\r\n",
            "LOCATION:Room 1\r\n",
            "URL:https://calendar.example/event\r\n",
        );

        let events = parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 7, 30, 0, 0), None).unwrap();

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.uid, "meeting");
        assert_eq!(event.summary.as_deref(), Some("Planning"));
        assert_eq!(event.description.as_deref(), Some("Roadmap review"));
        assert_eq!(event.location.as_deref(), Some("Room 1"));
        assert_eq!(event.url.as_deref(), Some("https://calendar.example/event"));
        assert_eq!(
            event.details_url.as_ref().map(Url::as_str),
            Some("https://calendar.example/event")
        );
        assert!(!event.is_recurring);
        assert_eq!(event.start_at, utc(2026, 7, 29, 16, 0));
        assert_eq!(event.end_at, utc(2026, 7, 29, 17, 0));
    }

    #[test]
    fn derives_best_effort_link_for_opted_in_one_time_google_event() {
        let data = event!(
            "UID:20260907_3iv7l7ja8ses3smgpgh5fqearo@google.com\r\n",
            "DTSTART:20260907T000000Z\r\n",
            "DTEND:20260908T000000Z\r\n",
            "SUMMARY:Labor Day\r\n",
        );
        let mut events =
            parse_events(data, utc(2026, 9, 7, 0, 0), utc(2026, 9, 9, 0, 0), None).unwrap();
        let source_url = Url::parse(concat!(
            "https://calendar.google.com/calendar/ical/",
            "en.usa%23holiday%40group.v.calendar.google.com/public/basic.ics",
        ))
        .unwrap();

        derive_google_event_details_urls(&mut events, &source_url);

        assert_eq!(
            events[0].details_url.as_ref().map(Url::as_str),
            Some(concat!(
                "https://calendar.google.com/calendar/event?eid=",
                "MjAyNjA5MDdfM2l2N2w3amE4c2VzM3NtZ3BnaDVmcWVhcm8g",
                "ZW4udXNhI2hvbGlkYXlAZ3JvdXAudi5jYWxlbmRhci5nb29nbGUuY29t",
            ))
        );
    }

    #[test]
    fn explicit_event_url_takes_precedence_over_google_details_url() {
        let data = event!(
            "UID:nativeevent@google.com\r\n",
            "DTSTART:20260729T160000Z\r\n",
            "DTEND:20260729T170000Z\r\n",
            "URL:https://calendar.example/event-details\r\n",
            "X-GOOGLE-CONFERENCE:https://meet.google.com/abc-defg-hij\r\n",
            "DESCRIPTION:Join Zoom Meeting: https://zoom.us/j/00000000000?pwd=dummy\r\n",
        );
        let mut events =
            parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 7, 30, 0, 0), None).unwrap();
        let source_url = Url::parse(
            "https://calendar.google.com/calendar/ical/user%40example.com/private-secret/basic.ics",
        )
        .unwrap();

        derive_google_event_details_urls(&mut events, &source_url);

        assert_eq!(
            events[0].details_url.as_ref().map(Url::as_str),
            Some("https://calendar.example/event-details")
        );
    }

    #[test]
    fn uses_google_meet_conference_url_for_recurring_instances() {
        let data = event!(
            "UID:seriesevent@google.com\r\n",
            "DTSTART;TZID=America/New_York:20260727T123000\r\n",
            "DTEND;TZID=America/New_York:20260727T131500\r\n",
            "RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR\r\n",
            "X-GOOGLE-CONFERENCE:https://meet.google.com/abc-defg-hij\r\n",
        );

        let events =
            parse_events(data, utc(2026, 8, 3, 16, 0), utc(2026, 8, 3, 17, 0), None).unwrap();

        assert_eq!(events.len(), 1);
        assert!(events[0].is_recurring);
        assert_eq!(events[0].start_at, utc(2026, 8, 3, 16, 30));
        assert_eq!(
            events[0].details_url.as_ref().map(Url::as_str),
            Some("https://meet.google.com/abc-defg-hij")
        );
    }

    #[test]
    fn rejects_non_meet_google_conference_urls() {
        let data = event!(
            "UID:meeting@google.com\r\n",
            "DTSTART:20260803T163000Z\r\n",
            "DTEND:20260803T171500Z\r\n",
            "X-GOOGLE-CONFERENCE:https://example.com/not-a-meeting\r\n",
        );

        let events =
            parse_events(data, utc(2026, 8, 3, 16, 0), utc(2026, 8, 3, 18, 0), None).unwrap();

        assert_eq!(events.len(), 1);
        assert!(events[0].details_url.is_none());
    }

    #[test]
    fn uses_zoom_join_url_from_recurring_event_description() {
        let data = event!(
            "UID:monthly@google.com\r\n",
            "DTSTART;TZID=America/New_York:20260804T123000\r\n",
            "DTEND;TZID=America/New_York:20260804T130000\r\n",
            "RRULE:FREQ=MONTHLY;BYDAY=1TU\r\n",
            "DESCRIPTION:Meeting agenda: https://docs.zoom.us/agenda/doc/example\\n\
             Join Zoom Meeting:\\nhttps://zoom.us/j/00000000000?pwd=dummy&jst=2\\n\
             Chat: https://zoom.us/launch/jc/00000000000\r\n",
        );

        let events =
            parse_events(data, utc(2026, 8, 4, 16, 0), utc(2026, 8, 4, 18, 0), None).unwrap();

        assert_eq!(events.len(), 1);
        assert!(events[0].is_recurring);
        assert_eq!(events[0].start_at, utc(2026, 8, 4, 16, 30));
        assert_eq!(
            events[0].details_url.as_ref().map(Url::as_str),
            Some("https://zoom.us/j/00000000000?pwd=dummy&jst=2")
        );
    }

    #[test]
    fn rejects_non_join_and_lookalike_zoom_description_urls() {
        for description in [
            "https://docs.zoom.us/agenda/doc/example",
            "https://zoom.us/launch/jc/00000000000",
            "https://zoom.us.example.com/j/00000000000",
            "http://zoom.us/j/00000000000",
            "https://zoom.us/j/not-a-number",
        ] {
            let data = format!(
                "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\n\
                 UID:meeting@google.com\r\nDTSTART:20260804T163000Z\r\n\
                 DTEND:20260804T170000Z\r\nDESCRIPTION:{description}\r\n\
                 END:VEVENT\r\nEND:VCALENDAR\r\n"
            );
            let events =
                parse_events(&data, utc(2026, 8, 4, 16, 0), utc(2026, 8, 4, 18, 0), None).unwrap();

            assert_eq!(events.len(), 1);
            assert!(events[0].details_url.is_none(), "accepted {description}");
        }
    }

    #[test]
    fn private_ics_secret_is_not_in_generated_details_url() {
        let data = event!(
            "UID:nativeevent@google.com\r\n",
            "DTSTART:20260729T160000Z\r\n",
            "DTEND:20260729T170000Z\r\n",
        );
        let mut events =
            parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 7, 30, 0, 0), None).unwrap();
        let source_url = Url::parse(
            "https://calendar.google.com/calendar/ical/user%40example.com/private-feed-secret/basic.ics",
        )
        .unwrap();

        derive_google_event_details_urls(&mut events, &source_url);

        let details_url = events[0].details_url.as_ref().unwrap().as_str();
        assert!(!details_url.contains("feed-secret"));
        assert!(!details_url.contains("private-"));
    }

    #[test]
    fn recognizes_percent_encoded_google_caldav_calendar_id() {
        let source_url = Url::parse(
            "https://apidata.googleusercontent.com/caldav/v2/user%40example.com/events/",
        )
        .unwrap();

        assert_eq!(
            google_calendar_id(&source_url).as_deref(),
            Some("user@example.com")
        );
    }

    #[test]
    fn filters_only_declines_from_the_google_calendar_owner() {
        let data = calendar!(
            "BEGIN:VEVENT\r\n",
            "UID:owner-declined\r\n",
            "DTSTART:20260803T160000Z\r\n",
            "DTEND:20260803T170000Z\r\n",
            "ATTENDEE;PARTSTAT=declined:MAILTO:USER@EXAMPLE.COM\r\n",
            "ATTENDEE;PARTSTAT=ACCEPTED:mailto:guest@example.com\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:guest-declined\r\n",
            "DTSTART:20260803T180000Z\r\n",
            "DTEND:20260803T190000Z\r\n",
            "ATTENDEE;PARTSTAT=ACCEPTED:mailto:user@example.com\r\n",
            "ATTENDEE;PARTSTAT=DECLINED:mailto:guest@example.com\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:unrelated-decline\r\n",
            "DTSTART:20260803T200000Z\r\n",
            "DTEND:20260803T210000Z\r\n",
            "ATTENDEE;PARTSTAT=DECLINED:mailto:guest@example.com\r\n",
            "END:VEVENT\r\n",
        );
        let source_url = Url::parse(
            "https://calendar.google.com/calendar/ical/user%40example.com/private-secret/basic.ics",
        )
        .unwrap();

        let filtered = parse_events(
            data,
            utc(2026, 8, 3, 15, 0),
            utc(2026, 8, 3, 22, 0),
            Some(&source_url),
        )
        .unwrap();
        let unfiltered =
            parse_events(data, utc(2026, 8, 3, 15, 0), utc(2026, 8, 3, 22, 0), None).unwrap();

        assert_eq!(
            filtered
                .iter()
                .map(|event| event.uid.as_str())
                .collect::<Vec<_>>(),
            ["guest-declined", "unrelated-decline"]
        );
        assert_eq!(unfiltered.len(), 3);
    }

    #[test]
    fn skips_non_google_sources_and_non_native_or_malformed_uids() {
        let native =
            best_effort_google_event_details_url("nativeevent@google.com", "user@example.com");
        let imported =
            best_effort_google_event_details_url("event@example.com", "user@example.com");
        let malformed =
            best_effort_google_event_details_url("bad event@google.com", "user@example.com");

        assert!(native.is_some());
        assert!(imported.is_none());
        assert!(malformed.is_none());
        assert!(google_calendar_id(
            &Url::parse("https://calendar.example/calendar/ical/user/public/basic.ics").unwrap()
        )
        .is_none());
    }

    #[test]
    fn does_not_derive_links_for_recurring_instances_or_moved_overrides() {
        let data = calendar!(
            "BEGIN:VEVENT\r\n",
            "UID:seriesevent@google.com\r\n",
            "DTSTART:20260729T160000Z\r\n",
            "DTEND:20260729T163000Z\r\n",
            "RRULE:FREQ=DAILY;COUNT=2\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:seriesevent@google.com\r\n",
            "RECURRENCE-ID:20260730T160000Z\r\n",
            "DTSTART:20260730T200000Z\r\n",
            "DTEND:20260730T203000Z\r\n",
            "END:VEVENT\r\n",
        );
        let mut events =
            parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 8, 1, 0, 0), None).unwrap();
        let source_url = Url::parse(
            "https://calendar.google.com/calendar/ical/user%40example.com/public/basic.ics",
        )
        .unwrap();

        derive_google_event_details_urls(&mut events, &source_url);

        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|event| event.is_recurring));
        assert!(events.iter().all(|event| event.details_url.is_none()));
    }

    #[test]
    fn expands_rrule_rdate_and_exdate() {
        let data = event!(
            "UID:daily\r\n",
            "DTSTART:20260729T160000Z\r\n",
            "DTEND:20260729T163000Z\r\n",
            "RRULE:FREQ=DAILY;COUNT=3\r\n",
            "EXDATE:20260730T160000Z\r\n",
            "RDATE:20260801T160000Z\r\n",
            "SUMMARY:Standup\r\n",
        );

        let events = parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 8, 2, 0, 0), None).unwrap();
        let starts: Vec<_> = events.iter().map(|event| event.start_at).collect();

        assert_eq!(
            starts,
            [
                utc(2026, 7, 29, 16, 0),
                utc(2026, 7, 31, 16, 0),
                utc(2026, 8, 1, 16, 0),
            ]
        );
    }

    #[test]
    fn expands_rdate_and_exdate_without_rrule() {
        let data = calendar!(
            "BEGIN:VEVENT\r\n",
            "UID:extra-date\r\n",
            "DTSTART:20260729T160000Z\r\n",
            "DTEND:20260729T163000Z\r\n",
            "RDATE:20260730T160000Z\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:excluded-date\r\n",
            "DTSTART:20260729T170000Z\r\n",
            "DTEND:20260729T173000Z\r\n",
            "EXDATE:20260729T170000Z\r\n",
            "END:VEVENT\r\n",
        );

        let events = parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 7, 31, 0, 0), None).unwrap();
        let starts: Vec<_> = events.iter().map(|event| event.start_at).collect();

        assert_eq!(starts, [utc(2026, 7, 29, 16, 0), utc(2026, 7, 30, 16, 0)]);
    }

    #[test]
    fn applies_moved_and_cancelled_recurrence_overrides() {
        let data = calendar!(
            "BEGIN:VEVENT\r\n",
            "UID:daily\r\n",
            "DTSTART:20260729T160000Z\r\n",
            "DTEND:20260729T163000Z\r\n",
            "RRULE:FREQ=DAILY;COUNT=3\r\n",
            "SUMMARY:Standup\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:daily\r\n",
            "RECURRENCE-ID:20260730T160000Z\r\n",
            "DTSTART:20260730T200000Z\r\n",
            "DTEND:20260730T203000Z\r\n",
            "SUMMARY:Moved standup\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:daily\r\n",
            "RECURRENCE-ID:20260731T160000Z\r\n",
            "STATUS:CANCELLED\r\n",
            "END:VEVENT\r\n",
        );

        let mut events =
            parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 8, 1, 0, 0), None).unwrap();
        events.sort_by_key(|event| event.start_at);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].start_at, utc(2026, 7, 29, 16, 0));
        assert_eq!(events[0].summary.as_deref(), Some("Standup"));
        assert_eq!(events[1].start_at, utc(2026, 7, 30, 20, 0));
        assert_eq!(events[1].summary.as_deref(), Some("Moved standup"));
    }

    #[test]
    fn uses_exclusive_all_day_end() {
        let data = event!(
            "UID:all-day\r\n",
            "DTSTART;VALUE=DATE:20260729\r\n",
            "DTEND;VALUE=DATE:20260730\r\n",
            "SUMMARY:All day\r\n",
        );

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 7, 31, 0, 0), None).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].end_at - events[0].start_at, Duration::days(1));
    }

    #[test]
    fn supports_duration_and_events_already_in_progress() {
        let data = event!(
            "UID:long\r\n",
            "DTSTART:20260728T180000Z\r\n",
            "DURATION:P2D\r\n",
            "SUMMARY:Long event\r\n",
        );

        let events = parse_events(data, utc(2026, 7, 29, 12, 0), utc(2026, 7, 30, 12, 0), None).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].start_at, utc(2026, 7, 28, 18, 0));
        assert_eq!(events[0].end_at, utc(2026, 7, 30, 18, 0));
    }

    #[test]
    fn rejects_malformed_calendar() {
        let error = parse_events(
            "not an iCalendar feed",
            utc(2026, 7, 29, 0, 0),
            utc(2026, 7, 30, 0, 0),
            None,
        )
        .unwrap_err();

        assert!(matches!(error, CalendarError::Parsing(_)));
    }

    #[test]
    fn isolates_invalid_recurrence_rules() {
        let data = calendar!(
            "BEGIN:VEVENT\r\n",
            "UID:broken-recurrence\r\n",
            "DTSTART:20260730T160000Z\r\n",
            "DTEND:20260730T170000Z\r\n",
            "RRULE:FREQ=DAILY;UNTIL=20260729T160000Z\r\n",
            "SUMMARY:Base occurrence\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:valid-event\r\n",
            "DTSTART:20260731T160000Z\r\n",
            "DTEND:20260731T170000Z\r\n",
            "SUMMARY:Valid event\r\n",
            "END:VEVENT\r\n",
        );

        let mut events =
            parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 8, 1, 0, 0), None).unwrap();
        events.sort_by_key(|event| event.start_at);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].summary.as_deref(), Some("Base occurrence"));
        assert_eq!(events[1].summary.as_deref(), Some("Valid event"));
    }

    #[test]
    fn parses_only_rfc5545_event_durations() {
        let duration = parse_duration("+P100DT2H3M4S").unwrap();
        assert_eq!(duration.days, 100);
        assert_eq!(duration.clock, Duration::seconds(7384));

        let weeks = parse_duration("P100W").unwrap();
        assert_eq!(weeks.days, 700);
        assert_eq!(weeks.clock, Duration::zero());

        for valid in ["PT1H", "PT1H2M", "PT1M", "PT1M2S", "PT1S", "P1DT1S"] {
            assert!(parse_duration(valid).is_ok(), "{valid} was rejected");
        }

        for invalid in [
            "-P1D",
            "P1Y",
            "P1M",
            "PT1.5S",
            "P1DT",
            "P1D1H",
            "PT1M1H",
            "PT1H2S",
            "P1DT1H2S",
            "P",
            "PT",
            "PT9223372036854775807S",
        ] {
            assert!(parse_duration(invalid).is_err(), "{invalid} was accepted");
        }
    }

    #[test]
    fn nominal_day_duration_follows_dst_transitions() {
        let spring = event!(
            "UID:spring\r\n",
            "DTSTART;TZID=America/Los_Angeles:20260307T090000\r\n",
            "DURATION:P1D\r\n",
        );
        let event = parse_events(spring, utc(2026, 3, 7, 0, 0), utc(2026, 3, 10, 0, 0), None)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(event.end_at - event.start_at, Duration::hours(23));

        let fall = event!(
            "UID:fall\r\n",
            "DTSTART;TZID=America/Los_Angeles:20261031T090000\r\n",
            "DURATION:P1D\r\n",
        );
        let event = parse_events(fall, utc(2026, 10, 31, 0, 0), utc(2026, 11, 3, 0, 0), None)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(event.end_at - event.start_at, Duration::hours(25));
    }

    #[test]
    fn default_all_day_duration_uses_next_local_midnight() {
        let data = event!("UID:all-day-spring\r\n", "DTSTART;VALUE=DATE:20260308\r\n",);
        let event = first_event(data);
        let (_, end) = event_bounds(&event).unwrap();

        assert!(matches!(
            end,
            EventEnd::Nominal {
                duration: IcalDuration {
                    days: 1,
                    clock
                },
                timezone: CalendarTimezone::Local,
            } if clock == Duration::zero()
        ));
    }

    #[test]
    fn malformed_events_do_not_reject_the_feed() {
        let data = calendar!(
            "BEGIN:VEVENT\r\n",
            "UID:bad-duration\r\n",
            "DTSTART:20260729T160000Z\r\n",
            "DURATION:P1M\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:negative-duration\r\n",
            "DTSTART:20260729T160000Z\r\n",
            "DURATION:-P1D\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:backwards\r\n",
            "DTSTART:20260729T170000Z\r\n",
            "DTEND:20260729T160000Z\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:unknown-zone\r\n",
            "DTSTART;TZID=Custom/Office:20260729T160000\r\n",
            "DTEND;TZID=Custom/Office:20260729T170000\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:valid\r\n",
            "DTSTART:20260730T160000Z\r\n",
            "DTEND:20260730T170000Z\r\n",
            "SUMMARY:Still parsed\r\n",
            "END:VEVENT\r\n",
        );

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 8, 1, 0, 0), None).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].uid, "valid");
    }

    #[test]
    fn rejects_invalid_dtend_without_rejecting_the_feed() {
        let data = calendar!(
            "BEGIN:VEVENT\r\n",
            "UID:mismatched-value-type\r\n",
            "DTSTART;VALUE=DATE:20260729\r\n",
            "DTEND:20260730T170000Z\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:mismatched-local-form\r\n",
            "DTSTART:20260729T160000\r\n",
            "DTEND:20260729T170000Z\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:equal-end\r\n",
            "DTSTART:20260729T160000Z\r\n",
            "DTEND:20260729T160000Z\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:valid\r\n",
            "DTSTART:20260730T160000Z\r\n",
            "DTEND:20260730T170000Z\r\n",
            "END:VEVENT\r\n",
        );

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 8, 1, 0, 0), None).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].uid, "valid");
    }

    #[test]
    fn skips_events_without_required_uid() {
        let data = calendar!(
            "BEGIN:VEVENT\r\n",
            "DTSTART:20260729T160000Z\r\n",
            "DTEND:20260729T170000Z\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:valid\r\n",
            "DTSTART:20260730T160000Z\r\n",
            "DTEND:20260730T170000Z\r\n",
            "END:VEVENT\r\n",
        );

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 8, 1, 0, 0), None).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].uid, "valid");
    }

    #[test]
    fn rejects_this_and_future_series() {
        let data = calendar!(
            "BEGIN:VEVENT\r\n",
            "UID:series\r\n",
            "DTSTART:20260729T160000Z\r\n",
            "DTEND:20260729T170000Z\r\n",
            "RRULE:FREQ=DAILY;COUNT=3\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:series\r\n",
            "RECURRENCE-ID;RANGE=THISANDFUTURE:20260730T160000Z\r\n",
            "DTSTART:20260730T180000Z\r\n",
            "DTEND:20260730T190000Z\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:independent\r\n",
            "DTSTART:20260730T200000Z\r\n",
            "DTEND:20260730T210000Z\r\n",
            "END:VEVENT\r\n",
        );

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 8, 2, 0, 0), None).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].uid, "independent");
    }

    #[test]
    fn skips_recurrences_that_would_require_excessive_fast_forwarding() {
        let data = calendar!(
            "BEGIN:VEVENT\r\n",
            "UID:pathological\r\n",
            "DTSTART:20000101T000000Z\r\n",
            "DURATION:PT1S\r\n",
            "RRULE:FREQ=SECONDLY\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:valid\r\n",
            "DTSTART:20260730T160000Z\r\n",
            "DTEND:20260730T170000Z\r\n",
            "END:VEVENT\r\n",
        );

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 8, 1, 0, 0), None).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].uid, "valid");
    }

    #[test]
    fn count_does_not_bypass_recurrence_work_limit() {
        let data = event!(
            "UID:pathological-with-count\r\n",
            "DTSTART:20000101T000000Z\r\n",
            "DURATION:PT1S\r\n",
            "RRULE:FREQ=SECONDLY;COUNT=100000;BYMINUTE=0;BYSECOND=0\r\n",
        );
        let event = first_event(data);
        let recurrence = event.get_recurrence().unwrap();

        assert_eq!(recurrence.get_rrule()[0].get_count(), Some(100_000));
        assert!(recurrence_requires_too_much_work(
            &recurrence,
            utc(2026, 7, 29, 0, 0)
        ));
    }

    #[test]
    fn recurrence_work_limit_includes_the_requested_window() {
        let data = event!(
            "UID:pathological-in-window\r\n",
            "DTSTART:20260729T000000Z\r\n",
            "DURATION:PT1S\r\n",
            "RRULE:FREQ=SECONDLY;BYMINUTE=0;BYSECOND=0\r\n",
        );
        let event = first_event(data);
        let recurrence = event.get_recurrence().unwrap();

        assert!(recurrence_requires_too_much_work(
            &recurrence,
            utc(2026, 8, 1, 0, 0)
        ));
    }

    #[test]
    fn search_window_end_is_exclusive() {
        let data = event!(
            "UID:at-end\r\n",
            "DTSTART:20260730T000000Z\r\n",
            "DTEND:20260730T010000Z\r\n",
        );

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 7, 30, 0, 0), None).unwrap();

        assert!(events.is_empty());
    }
}
