use chrono::format::{Fixed, Item, StrftimeItems};
use chrono::{DateTime, Datelike, Local, LocalResult, Locale, TimeZone, Timelike};
use chrono_tz::{OffsetName, Tz};

use std::fmt::Display;
use std::sync::LazyLock;

use super::*;

make_log_macro!(error, "datetime");

const DEFAULT_DATETIME_FORMAT: &str = "%a %d/%m %R";

pub static DEFAULT_DATETIME_FORMATTER: LazyLock<DatetimeFormatter> =
    LazyLock::new(|| DatetimeFormatter::new(Some(DEFAULT_DATETIME_FORMAT), None).unwrap());

#[derive(Debug)]
pub enum DatetimeFormatter {
    Chrono {
        items: Vec<Item<'static>>,
        locale: Option<Locale>,
    },
    #[cfg(feature = "icu_calendar")]
    Icu {
        length: icu_datetime::options::length::Date,
        locale: icu_locid::Locale,
    },
}

impl DatetimeFormatter {
    pub(super) fn from_args(args: &[Arg]) -> Result<Self> {
        let mut format = None;
        let mut locale = None;
        for arg in args {
            match arg.key {
                "format" | "f" => {
                    format = Some(arg.val);
                }
                "locale" | "l" => {
                    locale = Some(arg.val);
                }
                other => {
                    return Err(Error::new(format!(
                        "Unknown argument for 'datetime': '{other}'"
                    )));
                }
            }
        }
        Self::new(format, locale)
    }

    fn new(format: Option<&str>, locale: Option<&str>) -> Result<Self> {
        let (items, locale) = match locale {
            Some(locale) => {
                #[cfg(feature = "icu_calendar")]
                let Ok(locale) = locale.try_into() else {
                    use std::str::FromStr as _;
                    // try with icu4x
                    let locale = icu_locid::Locale::from_str(locale)
                        .ok()
                        .error("invalid locale")?;
                    let length = match format {
                        Some("full") => icu_datetime::options::length::Date::Full,
                        None | Some("long") => icu_datetime::options::length::Date::Long,
                        Some("medium") => icu_datetime::options::length::Date::Medium,
                        Some("short") => icu_datetime::options::length::Date::Short,
                        _ => return Err(Error::new("Unknown format option for icu based locale")),
                    };
                    return Ok(Self::Icu { locale, length });
                };
                #[cfg(not(feature = "icu_calendar"))]
                let locale = locale.try_into().ok().error("invalid locale")?;
                (
                    StrftimeItems::new_with_locale(
                        format.unwrap_or(DEFAULT_DATETIME_FORMAT),
                        locale,
                    ),
                    Some(locale),
                )
            }
            None => (
                StrftimeItems::new(format.unwrap_or(DEFAULT_DATETIME_FORMAT)),
                None,
            ),
        };

        Ok(Self::Chrono {
            items: items.parse_to_owned().error(format!(
                "Invalid format: \"{}\"",
                format.unwrap_or(DEFAULT_DATETIME_FORMAT)
            ))?,
            locale,
        })
    }
}

pub(crate) trait TimezoneName {
    fn timezone_name(datetime: &DateTime<Self>) -> Item
    where
        Self: TimeZone;
}

impl TimezoneName for Tz {
    fn timezone_name(datetime: &DateTime<Tz>) -> Item {
        Item::Literal(datetime.offset().abbreviation())
    }
}

impl TimezoneName for Local {
    fn timezone_name(datetime: &DateTime<Local>) -> Item {
        let fallback = Item::Fixed(Fixed::TimezoneName);
        let Ok(tz_name) = iana_time_zone::get_timezone() else {
            error!("Could not get local timezone");
            return fallback;
        };
        let tz = match tz_name.parse::<Tz>() {
            Ok(tz) => tz,
            Err(e) => {
                error!("{}", e);
                return fallback;
            }
        };

        match tz.with_ymd_and_hms(
            datetime.year(),
            datetime.month(),
            datetime.day(),
            datetime.hour(),
            datetime.minute(),
            datetime.second(),
        ) {
            LocalResult::Single(tz_datetime) => {
                Item::OwnedLiteral(tz_datetime.offset().abbreviation().into())
            }
            LocalResult::Ambiguous(..) => {
                error!("Timezone is ambiguous");
                fallback
            }
            LocalResult::None => {
                error!("Timezone is none");
                fallback
            }
        }
    }
}

fn borrow_item<'a>(item: &'a Item) -> Item<'a> {
    match item {
        Item::Literal(s) => Item::Literal(s),
        Item::OwnedLiteral(s) => Item::Literal(s),
        Item::Space(s) => Item::Space(s),
        Item::OwnedSpace(s) => Item::Space(s),
        Item::Numeric(n, p) => Item::Numeric(n.clone(), *p),
        Item::Fixed(f) => Item::Fixed(f.clone()),
        Item::Error => Item::Error,
    }
}

impl Formatter for DatetimeFormatter {
    fn format(&self, val: &Value, _config: &SharedConfig) -> Result<String, FormatError> {
        #[allow(clippy::unnecessary_wraps)]
        fn for_generic_datetime<T>(
            this: &DatetimeFormatter,
            datetime: DateTime<T>,
        ) -> Result<String, FormatError>
        where
            T: TimeZone + TimezoneName,
            T::Offset: Display,
        {
            Ok(match this {
                DatetimeFormatter::Chrono { items, locale } => {
                    let new_items = items.iter().map(|item| match item {
                        Item::Fixed(Fixed::TimezoneName) => T::timezone_name(&datetime),
                        item => borrow_item(item),
                    });
                    match *locale {
                        Some(locale) => datetime
                            .format_localized_with_items(new_items, locale)
                            .to_string(),
                        None => datetime.format_with_items(new_items).to_string(),
                    }
                }
                #[cfg(feature = "icu_calendar")]
                DatetimeFormatter::Icu { locale, length } => {
                    use chrono::Datelike as _;
                    let date = icu_calendar::Date::try_new_iso_date(
                        datetime.year(),
                        datetime.month() as u8,
                        datetime.day() as u8,
                    )
                    .ok()
                    .error("Current date should be a valid date")?;
                    let date = date.to_any();
                    let dft =
                        icu_datetime::DateFormatter::try_new_with_length(&locale.into(), *length)
                            .ok()
                            .error("locale should be present in compiled data")?;
                    dft.format_to_string(&date)
                        .ok()
                        .error("formatting date using icu failed")?
                }
            })
        }
        match val {
            Value::Datetime(datetime, timezone) => match timezone {
                Some(tz) => for_generic_datetime(self, datetime.with_timezone(tz)),
                None => for_generic_datetime(self, datetime.with_timezone(&Local)),
            },
            other => Err(FormatError::IncompatibleFormatter {
                ty: other.type_name(),
                fmt: "datetime",
            }),
        }
    }
}
