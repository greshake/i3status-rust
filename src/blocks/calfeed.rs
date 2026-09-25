//! Upcoming events from calfeed feeds
//!
//! [calfeed](https://calfeed.io) merges calendars into views and exposes each view as a credential-free feed URL. This block shows one event of one such view, and lets you step through the view's events and cycle between views. Feed URLs are secrets: pass them with the `url` configuration option, or a single one with the `I3RS_CALFEED_URL` environment variable.
//!
//! An event is `far`, `upcoming` (starts within `upcoming_threshold` minutes) or `running`; each phase maps to a [`State`], so the colours come from the theme (override them per block with `theme_overrides`).
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `url` | A feed URL from the calfeed console, or an array of them. Right click cycles between views. | `None`
//! `format` | A string to customise the output of this block. See below for available placeholders. | <code>\" $icon {$when $titles\|no events} \"</code>
//! `interval` | Update interval in seconds. Feeds are cached for 60 seconds upstream. | `60`
//! `upcoming_threshold` | Minutes before its start at which an event counts as upcoming | `15`
//! `far_state` | A valid [`State`] for events further away than `upcoming_threshold` | [`State::Idle`]
//! `upcoming_state` | A valid [`State`] for events about to start | [`State::Good`]
//! `running_state` | A valid [`State`] for events in progress | [`State::Info`]
//! `separator` | String to insert between the titles of concurrent events | `" \| "`
//! `max_titles` | How many titles `$titles` spells out before summarising the rest as `+N` | `2`
//! `hide_if_empty` | Hide this block when the current view has no events | `false`
//! `headers` | Extra request headers, for feeds that require one (`[block.headers]` table) | `{}`
//!
//! Placeholder    | Value                                                                     | Type     | Unit
//! ---------------|---------------------------------------------------------------------------|----------|-----
//! `icon`         | A static icon                                                             | Icon     | -
//! `when`         | `14:00`, `Tue 09:30` when not today, or `all day`, in the view's timezone | Text     | -
//! `title`        | Event title, or `busy` when the source shares no details                 | Text     | -
//! `titles`       | `$title`, then the titles of events overlapping it: `Doctor \| School run \| +1` | Text | -
//! `concurrent`   | Number of other events overlapping the shown one                          | Number   | -
//! `location`     | Event location, if shared                                                 | Text     | -
//! `start`        | Event start                                                               | Datetime | -
//! `end`          | Event end                                                                 | Datetime | -
//! `starts_in`    | Time until the event starts, zero once it is running                      | Duration | -
//! `duration`     | How long the event lasts                                                  | Duration | -
//! `all_day`      | Set when the event lasts whole days                                       | Flag     | -
//! `index`        | Position of the shown event in the view, starting at 1                    | Number   | -
//! `count`        | Number of events in the view                                              | Number   | -
//! `view`         | Name of the current view                                                  | Text     | -
//! `cur`          | The current view index                                                    | Number   | -
//! `avail`        | Total number of views to switch between                                   | Number   | -
//! `disconnected` | Number of the current view's sources that are disconnected                | Number   | -
//!
//! `when`, `title`, `titles`, `concurrent`, `location`, `start`, `end`, `starts_in`, `duration`, `all_day` and `index` are only set when the current view has at least one event.
//!
//! Action       | Default button
//! -------------|----------------
//! `next_event` | Left, Wheel Down
//! `prev_event` | Wheel Up
//! `next_view`  | Right
//! `prev_view`  | -
//!
//! # Examples
//!
//! ```toml
//! [[block]]
//! block = "calfeed"
//! url = "https://calfeed.io/v1/feed/..."
//! ```
//!
//! ```toml
//! [[block]]
//! block = "calfeed"
//! url = ["https://calfeed.io/v1/feed/...", "https://calfeed.io/v1/feed/..."]
//! format = " $icon $view {$when $titles.str(w:30) {$all_day{}|($duration.duration(min_unit:m, leading_zeroes:false, unit_space:true))} { · $location|}|no events} "
//! upcoming_threshold = 30
//! upcoming_state = "warning"
//! hide_if_empty = true
//! [block.headers]
//! X-Feed-Key = "..."
//! [block.theme_overrides]
//! warning_bg = "#d79921"
//! ```
//!
//! # Icons Used
//! - `calendar`

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use super::prelude::*;

const ICON: &str = "calendar";
const NO_TITLE: &str = "busy";
const ALL_DAY: &str = "all day";
const SOURCE_CONNECTED: &str = "connected";

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub url: FeedUrl,
    pub format: FormatConfig,
    #[default(60.into())]
    pub interval: Seconds,
    #[default(15)]
    pub upcoming_threshold: u64,
    pub far_state: Option<State>,
    pub upcoming_state: Option<State>,
    pub running_state: Option<State>,
    #[default(" | ".into())]
    pub separator: String,
    #[default(2)]
    pub max_titles: usize,
    pub hide_if_empty: bool,
    pub headers: HashMap<String, String>,
}

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(untagged)]
pub enum FeedUrl {
    Single(String),
    #[default]
    Multiple(Vec<String>),
}

/// One configured feed: its last fetch and which of its events is shown.
struct ViewState {
    url: String,
    feed: Option<Feed>,
    index: usize,
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let mut actions = api.get_actions()?;
    api.set_default_actions(&[
        (MouseButton::Left, None, "next_event"),
        (MouseButton::WheelDown, None, "next_event"),
        (MouseButton::WheelUp, None, "prev_event"),
        (MouseButton::Right, None, "next_view"),
    ])?;

    let format = config
        .format
        .with_default(" $icon {$when $titles|no events} ")?;

    let mut interval = config.interval.timer();
    let mut views: Vec<ViewState> = feed_urls(config)?
        .into_iter()
        .map(|url| ViewState {
            url,
            feed: None,
            index: 0,
        })
        .collect();
    let mut cur = 0;
    let upcoming = chrono::Duration::minutes(config.upcoming_threshold as i64);
    let state_of = |phase: Phase| match phase {
        Phase::Far => config.far_state.unwrap_or(State::Idle),
        Phase::Upcoming => config.upcoming_state.unwrap_or(State::Good),
        Phase::Running => config.running_state.unwrap_or(State::Info),
    };

    let mut refresh = true;
    loop {
        if refresh {
            for view in &mut views {
                let fetch = || get_feed(&view.url, &config.headers);
                let feed = fetch.retry(ExponentialBuilder::default()).await?;
                view.index = view.index.min(feed.events.len().saturating_sub(1));
                view.feed = Some(feed);
            }
            refresh = false;
        }

        let view = &views[cur];
        let feed = view.feed.as_ref().error("feed not fetched")?;
        let event = feed.events.get(view.index);

        if event.is_none() && config.hide_if_empty {
            api.hide()?;
        } else {
            let tz: Tz = feed
                .view
                .timezone
                .parse()
                .error("feed reports an unknown timezone")?;
            let now = Utc::now();

            let mut widget = Widget::new().with_format(format.clone());
            let mut values = map! {
                "icon" => Value::icon(ICON),
                "count" => Value::number(feed.events.len()),
                "view" => Value::text(feed.view.name.clone()),
                "cur" => Value::number(cur + 1),
                "avail" => Value::number(views.len()),
                "disconnected" => Value::number(feed.disconnected()),
            };
            if let Some(event) = event {
                let starts_in = (event.start - now).to_std().unwrap_or_default();
                widget.state = state_of(event.phase(now, upcoming));
                map! { @extend values
                    "when" => Value::text(event.when(now, tz)),
                    "title" => Value::text(event.title().to_string()),
                    "titles" => Value::text(feed.titles(view.index, &config.separator, config.max_titles)),
                    "concurrent" => Value::number(feed.concurrent(view.index).count()),
                    [if let Some(location) = &event.location] "location" => Value::text(location.clone()),
                    "start" => Value::datetime(event.start, Some(tz)),
                    "end" => Value::datetime(event.end, Some(tz)),
                    "starts_in" => Value::duration(starts_in),
                    "duration" => Value::duration((event.end - event.start).to_std().unwrap_or_default()),
                    "index" => Value::number(view.index + 1),
                    [if event.all_day] "all_day" => Value::flag(),
                }
            }
            widget.set_values(values);
            api.set_widget(widget)?;
        }

        select! {
            _ = interval.tick() => refresh = true,
            _ = api.wait_for_update_request() => refresh = true,
            Some(action) = actions.recv() => {
                let view = &mut views[cur];
                let count = view.feed.as_ref().map_or(0, |f| f.events.len());
                match action.as_ref() {
                    "next_event" => view.index = step(view.index, count, 1),
                    "prev_event" => view.index = step(view.index, count, -1),
                    "next_view" => cur = step(cur, views.len(), 1),
                    "prev_view" => cur = step(cur, views.len(), -1),
                    _ => (),
                }
            }
        }
    }
}

fn feed_urls(config: &Config) -> Result<Vec<String>> {
    let urls = match &config.url {
        FeedUrl::Single(url) => vec![url.clone()],
        FeedUrl::Multiple(urls) => urls.clone(),
    };
    if !urls.is_empty() {
        return Ok(urls);
    }
    let url = std::env::var("I3RS_CALFEED_URL").error("calfeed feed url not found")?;
    Ok(vec![url])
}

/// Move `index` by `by` within `count` items, wrapping around.
fn step(index: usize, count: usize, by: isize) -> usize {
    if count == 0 {
        return 0;
    }
    (index as isize + by).rem_euclid(count as isize) as usize
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Far,
    Upcoming,
    Running,
}

#[derive(Deserialize, Debug)]
struct Feed {
    view: View,
    sources: Vec<Source>,
    events: Vec<Event>,
}

#[derive(Deserialize, Debug)]
struct View {
    name: String,
    timezone: String,
}

#[derive(Deserialize, Debug)]
struct Source {
    status: String,
}

#[derive(Deserialize, Debug)]
struct Event {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    all_day: bool,
    title: Option<String>,
    location: Option<String>,
}

impl Feed {
    fn disconnected(&self) -> usize {
        self.sources
            .iter()
            .filter(|s| s.status != SOURCE_CONNECTED)
            .count()
    }

    /// The other events whose time overlaps event `shown`.
    fn concurrent(&self, shown: usize) -> impl Iterator<Item = &Event> {
        let main = &self.events[shown];
        self.events
            .iter()
            .enumerate()
            .filter(move |(i, e)| *i != shown && e.start < main.end && main.start < e.end)
            .map(|(_, e)| e)
    }

    /// `Doctor | School run | +1`: the shown event's title, the titles of
    /// events overlapping it up to `max` in total, and a count of the rest.
    fn titles(&self, shown: usize, separator: &str, max: usize) -> String {
        let mut titles = vec![self.events[shown].title()];
        let mut rest = 0;
        for event in self.concurrent(shown) {
            if titles.len() < max {
                titles.push(event.title());
            } else {
                rest += 1;
            }
        }
        let mut out = titles.join(separator);
        if rest > 0 {
            write!(out, "{separator}+{rest}").unwrap();
        }
        out
    }
}

impl Event {
    fn title(&self) -> &str {
        self.title.as_deref().unwrap_or(NO_TITLE)
    }

    fn phase(&self, now: DateTime<Utc>, upcoming: chrono::Duration) -> Phase {
        if self.start <= now {
            Phase::Running
        } else if self.start - now <= upcoming {
            Phase::Upcoming
        } else {
            Phase::Far
        }
    }

    /// The same wording the feed's `.txt` rendering uses: a weekday prefix
    /// only when the event is not today, then the start time or `all day`.
    /// All-day events are UTC midnight of their calendar date, so their
    /// date is read in UTC rather than shifted into the view's zone.
    fn when(&self, now: DateTime<Utc>, tz: Tz) -> String {
        let start = self.start.with_timezone(&tz);
        let date = if self.all_day {
            self.start.date_naive()
        } else {
            start.date_naive()
        };
        let mut when = String::new();
        if date != now.with_timezone(&tz).date_naive() {
            write!(when, "{} ", date.format("%a")).unwrap();
        }
        if self.all_day {
            when.push_str(ALL_DAY);
        } else {
            write!(when, "{}", start.format("%H:%M")).unwrap();
        }
        when
    }
}

async fn get_feed(url: &str, headers: &HashMap<String, String>) -> Result<Feed> {
    let mut request = REQWEST_CLIENT.get(url);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    request
        .send()
        .await
        .error("Failed to send request")?
        .error_for_status()
        .error("Feed request failed")?
        .json()
        .await
        .error("Failed to get JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(start: &str, end: &str, title: Option<&str>) -> Event {
        Event {
            start: start.parse().unwrap(),
            end: end.parse().unwrap(),
            all_day: false,
            title: title.map(Into::into),
            location: None,
        }
    }

    fn feed(events: Vec<Event>) -> Feed {
        Feed {
            view: View {
                name: "Today".into(),
                timezone: "UTC".into(),
            },
            sources: vec![
                Source {
                    status: "connected".into(),
                },
                Source {
                    status: "disconnected".into(),
                },
            ],
            events,
        }
    }

    #[test]
    fn url_accepts_one_or_many() {
        let one: Config = toml::from_str(r#"url = "https://a""#).unwrap();
        assert_eq!(feed_urls(&one).unwrap(), vec!["https://a"]);
        let many: Config = toml::from_str(r#"url = ["https://a", "https://b"]"#).unwrap();
        assert_eq!(feed_urls(&many).unwrap(), vec!["https://a", "https://b"]);
        let keyed: Config =
            toml::from_str("url = \"https://a\"\n[headers]\nX-Feed-Key = \"s\"").unwrap();
        assert_eq!(
            keyed.headers.get("X-Feed-Key").map(String::as_str),
            Some("s")
        );
    }

    #[test]
    fn stepping_wraps_and_tolerates_empty() {
        assert_eq!(step(0, 3, 1), 1);
        assert_eq!(step(2, 3, 1), 0);
        assert_eq!(step(0, 3, -1), 2);
        assert_eq!(step(0, 0, 1), 0);
    }

    #[test]
    fn phase_follows_the_threshold() {
        let now: DateTime<Utc> = "2026-09-08T10:00:00Z".parse().unwrap();
        let upcoming = chrono::Duration::minutes(15);
        let at = |start: &str| event(start, "2026-09-08T23:00:00Z", None).phase(now, upcoming);
        assert_eq!(at("2026-09-08T10:20:00Z"), Phase::Far);
        assert_eq!(at("2026-09-08T10:15:00Z"), Phase::Upcoming);
        assert_eq!(at("2026-09-08T10:00:00Z"), Phase::Running);
        assert_eq!(at("2026-09-08T09:00:00Z"), Phase::Running);
    }

    #[test]
    fn titles_list_overlapping_events_and_summarise_the_rest() {
        let f = feed(vec![
            event(
                "2026-09-08T10:00:00Z",
                "2026-09-08T11:00:00Z",
                Some("Doctor"),
            ),
            event(
                "2026-09-08T10:30:00Z",
                "2026-09-08T11:30:00Z",
                Some("School run"),
            ),
            event("2026-09-08T10:45:00Z", "2026-09-08T11:00:00Z", None),
            event(
                "2026-09-08T11:00:00Z",
                "2026-09-08T12:00:00Z",
                Some("Lunch"),
            ),
        ]);
        assert_eq!(f.titles(0, " | ", 2), "Doctor | School run | +1");
        assert_eq!(f.titles(0, " | ", 3), "Doctor | School run | busy");
        assert_eq!(f.titles(1, " / ", 2), "School run / Doctor / +2");
        assert_eq!(f.titles(3, " | ", 2), "Lunch | School run");
        assert_eq!(f.concurrent(0).count(), 2);
        assert_eq!(f.concurrent(3).count(), 1);
    }

    #[test]
    fn when_follows_the_txt_rendering() {
        let now: DateTime<Utc> = "2026-09-08T10:00:00Z".parse().unwrap();
        let tz: Tz = "Europe/Madrid".parse().unwrap();
        let at = |start: &str, all_day: bool| {
            Event {
                all_day,
                ..event(start, start, None)
            }
            .when(now, tz)
        };
        assert_eq!(at("2026-09-08T12:00:00Z", false), "14:00");
        assert_eq!(at("2026-09-09T07:30:00Z", false), "Wed 09:30");
        assert_eq!(at("2026-09-08T22:00:00Z", false), "Wed 00:00");
        assert_eq!(at("2026-09-08T00:00:00Z", true), "all day");
        assert_eq!(at("2026-09-09T00:00:00Z", true), "Wed all day");
    }

    #[test]
    fn all_day_events_keep_their_calendar_date() {
        // Stored as UTC midnight of the date; west of UTC that instant is
        // the evening before, and the label must not slip a day.
        let now: DateTime<Utc> = "2026-09-09T20:00:00Z".parse().unwrap();
        let tz: Tz = "America/Los_Angeles".parse().unwrap();
        let at = |start: &str| {
            Event {
                all_day: true,
                ..event(start, start, None)
            }
            .when(now, tz)
        };
        assert_eq!(at("2026-09-11T00:00:00Z"), "Fri all day");
        assert_eq!(at("2026-09-09T00:00:00Z"), "all day");
    }

    #[test]
    fn disconnected_counts_every_non_connected_source() {
        assert_eq!(feed(vec![]).disconnected(), 1);
    }
}
