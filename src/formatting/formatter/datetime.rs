use chrono::format::{Item, StrftimeItems};
use chrono::{DateTime, Local, Locale, TimeZone};
use once_cell::sync::Lazy;

use std::fmt::Display;

use super::*;

const DEFAULT_DATETIME_FORMAT: &str = "%a %d/%m %R";

pub static DEFAULT_DATETIME_FORMATTER: Lazy<DatetimeFormatter> =
    Lazy::new(|| DatetimeFormatter::new(Some(DEFAULT_DATETIME_FORMAT), None).unwrap());

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

fn make_static_item(item: Item<'_>) -> Item<'static> {
    match item {
        Item::Literal(str) => Item::OwnedLiteral(str.into()),
        Item::OwnedLiteral(boxed) => Item::OwnedLiteral(boxed),
        Item::Space(str) => Item::OwnedSpace(str.into()),
        Item::OwnedSpace(boxed) => Item::OwnedSpace(boxed),
        Item::Numeric(numeric, pad) => Item::Numeric(numeric, pad),
        Item::Fixed(fixed) => Item::Fixed(fixed),
        Item::Error => Item::Error,
    }
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
            items: items.map(make_static_item).collect(),
            locale,
        })
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
            T: TimeZone,
            T::Offset: Display,
        {
            Ok(match this {
                DatetimeFormatter::Chrono { items, locale } => match *locale {
                    Some(locale) => datetime.format_localized_with_items(items.iter(), locale),
                    None => datetime.format_with_items(items.iter()),
                }
                .to_string(),
                #[cfg(feature = "icu_calendar")]
                DatetimeFormatter::Icu { locale, length } => {
                    use chrono::Datelike as _;
                    let date = icu_calendar::Date::try_new_iso_date(
                        datetime.year_ce().1 as i32,
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
