use std::{
    collections::{HashMap, HashSet},
    str::FromStr as _,
};

use chrono::{DateTime, Days, Duration, Local, TimeZone as _, Utc};
use icalendar::{
    CalendarDateTime, Component as _, DatePerhapsTime, EventLike as _, EventStatus,
    Tz as RecurrenceTz,
};

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
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
}

pub fn parse_events(
    data: &str,
    event_search_start: DateTime<Utc>,
    event_search_end: DateTime<Utc>,
) -> Result<Vec<Event>, CalendarError> {
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
        let mut add_if_visible = |start, end| {
            if overlaps_search_window(start, end, event_search_start, event_search_end) {
                result.push(event_at(uid, event, start, end));
            }
        };

        if event.properties().contains_key("RECURRENCE-ID") {
            add_if_visible(event_start, event_end);
            continue;
        }

        let multi_properties = event.multi_properties();
        if !event.properties().contains_key("RRULE")
            && !multi_properties.contains_key("RDATE")
            && !multi_properties.contains_key("EXDATE")
        {
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
) -> Event {
    Event {
        uid: uid.into(),
        summary: event.get_summary().map(Into::into),
        description: event.get_description().map(Into::into),
        location: event.get_location().map(Into::into),
        url: event.get_url().map(Into::into),
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

        let events = parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 7, 30, 0, 0)).unwrap();

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.uid, "meeting");
        assert_eq!(event.summary.as_deref(), Some("Planning"));
        assert_eq!(event.description.as_deref(), Some("Roadmap review"));
        assert_eq!(event.location.as_deref(), Some("Room 1"));
        assert_eq!(event.url.as_deref(), Some("https://calendar.example/event"));
        assert_eq!(event.start_at, utc(2026, 7, 29, 16, 0));
        assert_eq!(event.end_at, utc(2026, 7, 29, 17, 0));
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

        let events = parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 8, 2, 0, 0)).unwrap();
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

        let events = parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 7, 31, 0, 0)).unwrap();
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
            parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 8, 1, 0, 0)).unwrap();
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

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 7, 31, 0, 0)).unwrap();

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

        let events = parse_events(data, utc(2026, 7, 29, 12, 0), utc(2026, 7, 30, 12, 0)).unwrap();

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
            parse_events(data, utc(2026, 7, 29, 15, 0), utc(2026, 8, 1, 0, 0)).unwrap();
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
        let event = parse_events(spring, utc(2026, 3, 7, 0, 0), utc(2026, 3, 10, 0, 0))
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(event.end_at - event.start_at, Duration::hours(23));

        let fall = event!(
            "UID:fall\r\n",
            "DTSTART;TZID=America/Los_Angeles:20261031T090000\r\n",
            "DURATION:P1D\r\n",
        );
        let event = parse_events(fall, utc(2026, 10, 31, 0, 0), utc(2026, 11, 3, 0, 0))
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

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 8, 1, 0, 0)).unwrap();

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

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 8, 1, 0, 0)).unwrap();

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

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 8, 1, 0, 0)).unwrap();

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

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 8, 2, 0, 0)).unwrap();

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

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 8, 1, 0, 0)).unwrap();

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

        let events = parse_events(data, utc(2026, 7, 29, 0, 0), utc(2026, 7, 30, 0, 0)).unwrap();

        assert!(events.is_empty());
    }
}
