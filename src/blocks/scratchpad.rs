//! Scratchpad indicator
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format`          | A string to customise the output of this block | ` $icon $count.eng(range:1..) |`
//!
//! Placeholder | Value                                      | Type   | Unit
//! ------------|--------------------------------------------|--------|-----
//! `icon`      | A static icon                              | Icon   | -
//! `count`     | Number of windows in scratchpad            | Number | -
//!
//! # Example
//!
//! ```toml
//! [[block]]
//! block = "scratchpad"
//! ```
//!
//! # Icons Used
//! - `scratchpad`

use swayipc_async::{Connection, Event as SwayEvent, EventType, Node, WindowChange};

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
}

fn count_scratchpad_windows(node: &Node) -> usize {
    node.find_as_ref(|n| n.name.as_deref() == Some("__i3_scratch"))
        .map_or(0, |node| node.floating_nodes.len())
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config
        .format
        .with_default(" $icon $count.eng(range:1..) |")?;

    let connection_for_events = Connection::new()
        .await
        .error("failed to open connection with swayipc")?;

    let mut connection_for_tree = Connection::new()
        .await
        .error("failed to open connection with swayipc")?;

    let mut events = connection_for_events
        .subscribe(&[EventType::Window])
        .await
        .error("could not subscribe to window events")?;

    loop {
        let mut widget = Widget::new().with_format(format.clone());

        let root_node = connection_for_tree
            .get_tree()
            .await
            .error("could not get windows tree")?;
        let count = count_scratchpad_windows(&root_node);

        widget.set_values(map! {
            "icon" => Value::icon("scratchpad"),
            "count" => Value::number(count),
        });

        api.set_widget(widget)?;

        loop {
            let event = events
                .next()
                .await
                .error("swayipc channel closed")?
                .error("bad event")?;

            match event {
                SwayEvent::Window(e) if e.change == WindowChange::Move => break,
                _ => continue,
            }
        }
    }
}
