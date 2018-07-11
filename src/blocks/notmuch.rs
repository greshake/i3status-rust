#![allow(non_camel_case_types)]

use chan::Sender;
use std::time::Duration;

use block::{Block, ConfigBlock};
use config::Config;
use de::deserialize_duration;
use errors::*;
use input::I3BarEvent;
use scheduler::Task;
use widget::I3BarWidget;
use widgets::text::TextWidget;

extern crate libc;
use self::libc::c_char;
use std::env;
use std::ffi::CString;
use std::ptr;
use std::result;
use widget::State;

use uuid::Uuid;

#[repr(C)]
pub struct notmuch_query_t;
pub struct notmuch_database_t;

// Status codes used for the return values of most functions.
///
/// A zero value (SUCCESS) indicates that the function completed without error. Any other value
/// indicates an error.
#[repr(C)]
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum notmuch_status_t {
    /// No error occurred.
    SUCCESS = 0,
    /// Out of memory.
    OUT_OF_MEMORY,
    /// An attempt was made to write to a database opened in read-only
    /// mode.
    READ_ONLY_DATABASE,
    /// A Xapian exception occurred.
    ///
    /// @todo We don't really want to expose this lame XAPIAN_EXCEPTION
    /// value. Instead we should map to things like DATABASE_LOCKED or
    /// whatever.
    XAPIAN_EXCEPTION,
    /// An error occurred trying to read or write to a file (this could
    /// be file not found, permission denied, etc.)
    FILE_ERROR,
    /// A file was presented that doesn't appear to be an email
    /// message.
    FILE_NOT_EMAIL,
    /// A file contains a message ID that is identical to a message
    /// already in the database.
    DUPLICATE_MESSAGE_ID,
    /// The user erroneously passed a NULL pointer to a notmuch
    /// function.
    NULL_POINTER,
    /// A tag value is too long (exceeds TAG_MAX).
    TAG_TOO_LONG,
    /// The `notmuch_message_thaw` function has been called more times
    /// than `notmuch_message_freeze`.
    UNBALANCED_FREEZE_THAW,
    /// `notmuch_database_end_atomic` has been called more times than
    /// `notmuch_database_begin_atomic`.
    UNBALANCED_ATOMIC,
    /// The operation is not supported.
    UNSUPPORTED_OPERATION,
    /// The operation requires a database upgrade.
    UPGRADE_REQUIRED,
    /// There is a problem with the proposed path, e.g. a relative path
    /// passed to a function expecting an absolute path.
    PATH_ERROR,
    /// One of the arguments violates the preconditions for the
    /// function, in a way not covered by a more specific argument.
    NOTMUCH_STATUS_ILLEGAL_ARGUMENT,
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum notmuch_database_mode_t {
    NOTMUCH_DATABASE_MODE_READ_ONLY = 0,
    NOTMUCH_DATABASE_MODE_READ_WRITE,
}

#[link(name = "notmuch")]
extern "C" {
    pub fn notmuch_query_count_messages(query: *mut notmuch_query_t, count: *mut u16) -> notmuch_status_t;

    pub fn notmuch_query_create(database: *mut notmuch_database_t, query_string: *const c_char) -> *mut notmuch_query_t;

    pub fn notmuch_database_open(path: *const c_char, mode: notmuch_database_mode_t, database: *mut *mut notmuch_database_t) -> notmuch_status_t;

    pub fn notmuch_database_destroy(database: *mut notmuch_database_t) -> notmuch_status_t;

}

pub struct Notmuch {
    text: TextWidget,
    id: String,
    update_interval: Duration,
    query: CString,
    db: CString,
    threshold_info: u16,
    threshold_good: u16,
    threshold_warning: u16,
    threshold_critical: u16,
    name: Option<String>,
    no_icon: bool,

    //useful, but optional
    #[allow(dead_code)]
    config: Config,
    #[allow(dead_code)]
    tx_update_request: Sender<Task>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct NotmuchConfig {
    /// Update interval in seconds
    #[serde(default = "NotmuchConfig::default_interval", deserialize_with = "deserialize_duration")]
    pub interval: Duration,
    #[serde(default = "NotmuchConfig::default_maildir")]
    pub maildir: String,
    #[serde(default = "NotmuchConfig::default_query")]
    pub query: String,
    #[serde(default = "NotmuchConfig::default_threshold_warning")]
    pub threshold_warning: u16,
    #[serde(default = "NotmuchConfig::default_threshold_critical")]
    pub threshold_critical: u16,
    #[serde(default = "NotmuchConfig::default_threshold_info")]
    pub threshold_info: u16,
    #[serde(default = "NotmuchConfig::default_threshold_good")]
    pub threshold_good: u16,
    #[serde(default = "NotmuchConfig::default_name")]
    pub name: Option<String>,
    #[serde(default = "NotmuchConfig::default_no_icon")]
    pub no_icon: bool,
}

impl NotmuchConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(10)
    }

    fn default_maildir() -> String {
        let home_dir = match env::home_dir() {
            Some(path) => path.into_os_string().into_string().unwrap(),
            None => "".to_owned(),
        };

        format!("{}/.mail", home_dir)
    }

    fn default_query() -> String {
        "".to_owned()
    }

    fn default_threshold_info() -> u16 {
        <u16>::max_value()
    }

    fn default_threshold_good() -> u16 {
        <u16>::max_value()
    }

    fn default_threshold_warning() -> u16 {
        <u16>::max_value()
    }

    fn default_threshold_critical() -> u16 {
        <u16>::max_value()
    }

    fn default_name() -> Option<String> {
        None
    }
    fn default_no_icon() -> bool {
        false
    }
}

fn run_query(db_path: &CString, query: &CString) -> result::Result<u16, notmuch_status_t> {
    let mut db = ptr::null_mut();
    let mut result = 0u16;
    unsafe {
        match notmuch_database_open(db_path.as_ptr(), notmuch_database_mode_t::NOTMUCH_DATABASE_MODE_READ_WRITE, &mut db) {
            notmuch_status_t::SUCCESS => {
                let query_ptr = notmuch_query_create(db, query.as_ptr());
                let p_result: *mut u16 = &mut result;
                notmuch_query_count_messages(query_ptr, &mut *p_result);
                notmuch_database_destroy(db);
                Ok(result)
            }
            status => Err(status),
        }
    }
}

impl ConfigBlock for Notmuch {
    type Config = NotmuchConfig;

    fn new(block_config: Self::Config, config: Config, tx_update_request: Sender<Task>) -> Result<Self> {
        let db_c_str = CString::new(block_config.maildir).unwrap();
        let query_c_str = CString::new(block_config.query).unwrap();

        let mut widget = TextWidget::new(config.clone());
        if !block_config.no_icon {
            widget.set_icon("mail");
        }
        Ok(Notmuch {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: block_config.interval,
            db: db_c_str,
            query: query_c_str,
            threshold_info: block_config.threshold_info,
            threshold_good: block_config.threshold_good,
            threshold_warning: block_config.threshold_warning,
            threshold_critical: block_config.threshold_critical,
            name: block_config.name,
            no_icon: block_config.no_icon,

            text: widget,
            tx_update_request: tx_update_request,
            config: config,
        })
    }
}

impl Notmuch {
    fn update_text(&mut self, count: u16) {
        let text = match self.name {
            Some(ref s) => format!("{}:{}", s, count),
            _ => format!("{}", count),
        };
        self.text.set_text(text);
    }

    fn update_state(&mut self, count: u16) {
        let mut state = { State::Idle };
        if count >= self.threshold_critical {
            state = { State::Critical };
        } else if count >= self.threshold_warning {
            state = { State::Warning };
        } else if count >= self.threshold_good {
            state = { State::Good };
        } else if count >= self.threshold_info {
            state = { State::Info };
        }
        self.text.set_state(state);
    }
}

impl Block for Notmuch {
    fn update(&mut self) -> Result<Option<Duration>> {
        match run_query(&self.db, &self.query) {
            Err(_) => Err(BlockError("foo".to_owned(), "bar".to_owned())),
            Ok(count) => {
                self.update_text(count);
                self.update_state(count);
                Ok(Some(self.update_interval))
            }
        }
    }

    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}
