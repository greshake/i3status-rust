use std::borrow::Cow;
use std::sync::Arc;

use i3status_rs::config::SharedConfig;
use i3status_rs::formatting::value::{Value, ValueInner};

// These constructors are part of the published library API.  The block-plan
// implementation may use stricter crate-private constructors, but adding an
// opaque in-crate capability argument to these methods is a breaking change
// for downstream callers.
#[test]
fn icon_value_constructors_remain_source_compatible() {
    let _ = Value::icon("time");
    let _ = Value::icon_progression("volume", 0.5);
    let _ = Value::icon_progression_bound("thermometer", 25.0, 0.0, 100.0);
}

#[test]
fn icon_value_payload_remains_source_compatible() {
    let icon = ValueInner::Icon(Cow::Borrowed("time"), None);
    let ValueInner::Icon(name, None) = icon else {
        panic!("constructed a non-icon value");
    };
    assert_eq!(name, "time");
}

#[test]
fn shared_config_remains_constructible_from_its_public_fields() {
    let _ = SharedConfig {
        theme: Arc::default(),
        icons: Arc::default(),
        icons_format: Arc::new("{icon}".to_string()),
    };
}
