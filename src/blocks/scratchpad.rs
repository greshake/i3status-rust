use swayipc_async::{Connection, Event as SwayEvent, EventType, Node, WindowChange};

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub format: FormatConfig,
    #[default(true)]
    pub hide_when_empty: bool,
}

fn count_scratchpad_windows(node: &Node) -> usize {
    node.find_as_ref(|n| n.name == Some("__i3_scratch".to_string()))
        .map(|node| node.floating_nodes.len())
        .unwrap_or(0)
}

pub async fn run(config: &Config, api: &CommonApi) -> Result<()> {
    let format = config.format.with_default("   $count ")?;

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

        widget.state = State::Idle;
        widget.set_values(map! {
            "icon" => Value::icon("cogs"), // #TODO 
            "count" => Value::number(count),
        });

        if count == 0 && config.hide_when_empty {
            api.hide()?;
        } else {
            api.set_widget(widget)?;
        }

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
