use chrono::format::{Fixed, Item, StrftimeItems};
use chrono::{DateTime, Local, Locale, TimeZone};
use chrono_tz::{OffsetName as _, Tz};

use std::fmt::Display;
use std::sync::LazyLock;

use super::*;

make_log_macro!(error, "datetime");

const DEFAULT_DATETIME_FORMAT: &str = "%a %d/%m %R";

pub static DEFAULT_DATETIME_FORMATTER: LazyLock<DatetimeFormatter> =
    LazyLock::new(|| DatetimeFormatter::new(Some(DEFAULT_DATETIME_FORMAT), None, None).unwrap());

#[derive(Debug)]
pub enum DatetimeFormatter {
    Chrono {
        items: Vec<Item<'static>>,
        locale: Option<Locale>,
    },
    #[cfg(feature = "icu_calendar")]
    Icu {
        fieldset: icu_datetime::fieldsets::enums::CompositeDateTimeFieldSet,
        locale: icu_locale_core::Locale,
    },
}

impl DatetimeFormatter {
    pub(super) fn from_args(args: &[Arg]) -> Result<Self> {
        let mut format = None;
        let mut locale = None;
        let mut precision = None;
        for arg in args {
            match arg.key {
                "format" | "f" => {
                    format = Some(arg.val.error("format must be specified")?);
                }
                "locale" | "l" => {
                    locale = Some(arg.val.error("locale must be specified")?);
                }
                "precision" | "p" => {
                    precision = Some(arg.val.error("precision must be specified")?);
                }
                other => {
                    return Err(Error::new(format!(
                        "Unknown argument for 'datetime': '{other}'"
                    )));
                }
            }
        }
        Self::new(format, locale, precision)
    }

    fn new(format: Option<&str>, locale: Option<&str>, precision: Option<&str>) -> Result<Self> {
        let (items, locale) = match locale {
            Some(locale) => {
                #[cfg(feature = "icu_calendar")]
                let Ok(locale) = locale.try_into() else {
                    // try with icu4x
                    use icu_datetime::fieldsets::{
                        self,
                        enums::{CompositeDateTimeFieldSet, DateAndTimeFieldSet, DateFieldSet},
                    };
                    use icu_datetime::options::{Length, TimePrecision};
                    use std::str::FromStr as _;

                    let precision = match precision {
                        Some("seconds" | "second" | "s") => Some(TimePrecision::Second),
                        Some("minutes" | "minute" | "m") => Some(TimePrecision::Minute),
                        Some("hours" | "hour" | "h") => Some(TimePrecision::Hour),
                        None => None,
                        _ => Err(Error::new("Invalid precision value"))?,
                    };
                    let locale = icu_locale_core::Locale::from_str(locale)
                        .ok()
                        .error("invalid locale")?;
                    let fieldset = match format {
                        Some("full") => match precision {
                            Some(precision) => {
                                CompositeDateTimeFieldSet::DateTime(DateAndTimeFieldSet::YMDET(
                                    fieldsets::YMDET::long().with_time_precision(precision),
                                ))
                            }
                            None => CompositeDateTimeFieldSet::Date(DateFieldSet::YMDE(
                                fieldsets::YMDE::long(),
                            )),
                        },
                        length => {
                            let length = match length {
                                Some("short") => Length::Short,
                                Some("medium") => Length::Medium,
                                Some("long") | None => Length::Long,
                                _ => Err(Error::new("Invalid length value"))?,
                            };
                            match precision {
                                Some(precision) => {
                                    CompositeDateTimeFieldSet::DateTime(DateAndTimeFieldSet::YMDT(
                                        fieldsets::YMDT::for_length(length)
                                            .with_time_precision(precision),
                                    ))
                                }
                                None => CompositeDateTimeFieldSet::Date(DateFieldSet::YMD(
                                    fieldsets::YMD::for_length(length),
                                )),
                            }
                        }
                    };

                    return Ok(Self::Icu { locale, fieldset });
                };
                #[cfg(not(feature = "icu_calendar"))]
                let locale = locale.try_into().ok().error("invalid locale")?;
                if precision.is_some() {
                    return Err(Error::new(
                        "`precision` is only available for icu datetimes",
                    ));
                }
                (
                    StrftimeItems::new_with_locale(
                        format.unwrap_or(DEFAULT_DATETIME_FORMAT),
                        locale,
                    ),
                    Some(locale),
                )
            }
            None => {
                if precision.is_some() {
                    return Err(Error::new(
                        "`precision` is only available for icu datetimes",
                    ));
                }
                (
                    StrftimeItems::new(format.unwrap_or(DEFAULT_DATETIME_FORMAT)),
                    None,
                )
            }
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
    fn timezone_name(datetime: &DateTime<Self>) -> Result<Item<'_>>
    where
        Self: TimeZone;
}

impl TimezoneName for Tz {
    fn timezone_name(datetime: &DateTime<Tz>) -> Result<Item<'_>> {
        Ok(Item::Literal(
            datetime
                .offset()
                .abbreviation()
                .error("Timezone name unknown")?,
        ))
    }
}

impl TimezoneName for Local {
    fn timezone_name(datetime: &DateTime<Local>) -> Result<Item<'_>> {
        let tz_name = iana_time_zone::get_timezone().error("Could not get local timezone")?;
        let tz = tz_name
            .parse::<Tz>()
            .error("Could not parse local timezone")?;
        Tz::timezone_name(&datetime.with_timezone(&tz)).map(|x| x.to_owned())
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
                        Item::Fixed(Fixed::TimezoneName) => match T::timezone_name(&datetime) {
                            Ok(name) => name,
                            Err(e) => {
                                error!("{e}");
                                Item::Fixed(Fixed::TimezoneName)
                            }
                        },
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
                DatetimeFormatter::Icu {
                    locale,
                    fieldset: length,
                } => {
                    use chrono::{Datelike as _, Timelike as _};
                    let datetime = icu_datetime::input::DateTime {
                        date: icu_datetime::input::Date::try_new_iso(
                            datetime.year(),
                            datetime.month() as u8,
                            datetime.day() as u8,
                        )
                        .error("Current date should be a valid date")?,
                        time: icu_datetime::input::Time::try_new(
                            datetime.hour() as u8,
                            datetime.minute() as u8,
                            datetime.second() as u8,
                            datetime.nanosecond(),
                        )
                        .error("Current time should be a valid time")?,
                    };
                    let dft = icu_datetime::DateTimeFormatter::try_new(locale.into(), *length)
                        .ok()
                        .error("locale should be present in compiled data")?;
                    dft.format(&datetime).to_string()
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
