use std::collections::{LinkedList, BTreeMap};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::util::pseudo_uuid;
use crate::widget::{I3BarWidget, State};

use std::thread;
use swayipc::reply::Event;
use swayipc::reply::WorkspaceChange;
use swayipc::{Connection, EventType};
use std::sync::{Arc, Mutex};
use crate::widgets::button::ButtonWidget;

//{{{struct WsKey

use std::cmp::Ordering;
#[derive(Eq)]
struct WsKey {
	num:     Option<i32>,
	name:    Option<String>,
}

//{{{
impl WsKey {
	//{{{
	fn new(num_opt: Option<i32>, name_opt: Option<String>) -> WsKey
	{
		match (num_opt, name_opt) {
			(None,      None)                                  => unreachable!(),
			(None,      Some(name))                            => WsKey{ num:  None,      name: Some(name) },
			(Some(num), None)                                  => WsKey{ num:  Some(num), name: None },
			(Some(num), Some(name)) if num == -1               => WsKey{ num:  None,      name: Some(name) },
			(Some(num), Some(name)) if num.to_string() == name => WsKey{ num:  Some(num), name: None },
			(Some(num), Some(name))                            => WsKey{ num:  Some(num), name: Some(name) },
			//(Some(num), Some(name))                            => WsKey{ num:  Some(num), name: Some(name.strip_prefix(&num.to_string()).unwrap_or_else(|| &name).to_string()) },
		}
	}
	//}}}
}
//}}}

//{{{
impl Ord for WsKey {
    fn cmp(&self, other: &Self) -> Ordering {
		// Goal (num, _) < (num, name) < (_, name)
		// priority for (num, name) is num

		let mut comp = match (self.num, other.num)
		{
			(Some(_), None)    => -100,
			(Some(s), Some(o)) => if s<o {-10} else if s>o {10} else {0},
			(None,    Some(_)) => 100,
			(None,    None)    =>  0,
		};

		comp += if self.name<other.name {-1} else if self.name==other.name {0} else {1};
		return comp.cmp(&0)
    }
}
//}}}

//{{{
impl PartialOrd for WsKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
//}}}

//{{{
impl PartialEq for WsKey {
    fn eq(&self, other: &Self) -> bool {
		if let Some(s) = self.num
		{
			if let Some(o) = other.num
			{
				s==o
			}
			else
			{
				false
			}
		}
		else
		{
			if let Some(_) = other.num
			{
				false
			}
			else
			{
				self.name==other.name
			}
		}
    }
}
//}}}
//}}}

//{{{
struct WS {
	urgent:  bool,
	focused: bool,
	config: Config,
}
//}}}

//{{{
pub struct Workspaces {
    id: String,
    update_interval: Duration,

	sway_connection: swayipc::Connection,

    workspaces: Arc<Mutex<BTreeMap<WsKey, WS>>>,
	ws_buttons: LinkedList<ButtonWidget>,

    //useful, but optional
    #[allow(dead_code)]
    config: Config,
    //#[allow(dead_code)]
    //tx_update_request: Sender<Task>,
}
//}}}

//{{{ pub struct WorkspacesConfig {

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct WorkspacesConfig {
    ///// Truncates titles if longer than max-width
    //#[serde(default = "WorkspacesConfig::default_max_width")]
    //pub max_width: usize,

    ///// Show marks in place of title (if exist)
    //#[serde(default = "WorkspacesConfig::default_show_marks")]
    //pub show_marks: bool,

    /// Update interval in seconds
    #[serde(
        default = "WorkspacesConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    #[serde(default = "WorkspacesConfig::default_color_overrides")]
    pub color_overrides: Option<BTreeMap<String, String>>,
}
//}}}

//{{{
impl WorkspacesConfig {
    fn default_interval() -> Duration {
        Duration::from_secs(5)
    }

    fn default_color_overrides() -> Option<BTreeMap<String, String>> {
        None
    }
}
//}}}

//{{{
impl ConfigBlock for Workspaces {
    type Config = WorkspacesConfig;

	//{{{
    fn new( block_config: Self::Config, config: Config, tx_update_request: Sender<Task>,) -> Result<Self>
	{
        let id: String = pseudo_uuid();
        let id_clone = id.clone();

		let workspaces_original = Arc::new(Mutex::new(BTreeMap::new()));
		let workspaces = workspaces_original.clone();

		let ws_buttons = LinkedList::new();

		let config_clone = config.clone();

        let mut sway_connection = Connection::new().block_error("workspaces", "failed to acquire connect to IPC")?;

		//{{{ Add initial workspaces
		{
			let mut workspaces = workspaces_original.lock().unwrap();
			for workspace in sway_connection.get_workspaces().unwrap()
			{
				workspaces.insert(
					WsKey::new(Some(workspace.num), Some(workspace.name)),
					WS {
						urgent: workspace.urgent,
						focused: workspace.focused,
						config: config_clone.clone()
					}
				);
			}
		}
		//}}}

		//{{{
        thread::Builder::new()
            .name("workspaces".into())
            .spawn(move || {
                for event in Connection::new()
                    .unwrap()
                    .subscribe(&[EventType::Workspace])
                    .unwrap()
                {
                    match event.unwrap() {
                        Event::Workspace(e) => {
                            match e.change {
								//{{{
								WorkspaceChange::Init   => {
                                    if let Some(ws_current) = e.current {

										let mut workspaces = workspaces_original.lock().unwrap();

										workspaces.insert(
											WsKey::new(ws_current.num, ws_current.name),
											WS {
												urgent: ws_current.urgent,
												focused: ws_current.focused,
												config: config_clone.clone()
											}
										);
                                    }
                                    tx_update_request.send(Task {
                                        id: id_clone.clone(),
                                        update_time: Instant::now(),
                                    })
                                    .unwrap();
								}
								//}}}
								//{{{
								WorkspaceChange::Empty  => {
                                    if let Some(ws_current) = e.current {

										let mut workspaces = workspaces_original.lock().unwrap();

										workspaces.remove(&WsKey::new(ws_current.num, ws_current.name));
									}
                                    tx_update_request.send(Task {
                                        id: id_clone.clone(),
                                        update_time: Instant::now(),
                                    })
                                    .unwrap();
								}
								//}}}
								//{{{
								WorkspaceChange::Focus  => {
									let mut workspaces = workspaces_original.lock().unwrap();

                                    if let Some(ws_current) = e.current {
										if let Some(ws) = workspaces.get_mut(&WsKey::new(ws_current.num, ws_current.name)) {
											ws.focused = true;
										}
									}

                                    if let Some(ws_old) = e.old {
										if let Some(ws) = workspaces.get_mut(&WsKey::new(ws_old.num, ws_old.name)) {
											ws.focused = false;
										}
									}


                                    tx_update_request.send(Task {
                                        id: id_clone.clone(),
                                        update_time: Instant::now(),
                                    })
                                    .unwrap();
								}
								//}}}
								//{{{
								WorkspaceChange::Move   => {
								}
								//}}}
								//{{{
								WorkspaceChange::Rename => {
								}
								//}}}
								//{{{
								WorkspaceChange::Urgent => {
								}
								//}}}
								//{{{
								WorkspaceChange::Reload => {
								}
								//}}}
                            };
                        }
                        _ => unreachable!(),
                        //_ => {},
                    }
                }
            })
            .unwrap();
		//}}}

        Ok(Workspaces {
            id: id.clone(),
            update_interval: block_config.interval,
            //tx_update_request,
			sway_connection: sway_connection,
            config: config,
			workspaces: workspaces,
			ws_buttons: ws_buttons,
        })
    }
	//}}}
}
//}}}

//{{{
impl Block for Workspaces {
	//{{{
    fn update(&mut self) -> Result<Option<Update>> {
		let workspaces = &(*self.workspaces.lock().block_error("workspaces", "failed to aquire lock")?);

		self.ws_buttons.clear();

		for (idx, ws) in workspaces
		{
			// TODO: use the strim_prefix() if option strip_ws_numbers is set
			let button_text = &if let Some(name) = &idx.name { name.clone() } else if let Some(num) = idx.num { num.to_string() } else { "INVALID".to_string() };

			let button_id = self.id.clone() + "_" +
				&(if let Some(name) = &idx.name { name.to_string() } else if let Some(num) = idx.num { num.to_string() } else { "INVALID".to_string() });

			self.ws_buttons.push_back(
				ButtonWidget::new(ws.config.clone(), &button_id)
					//.with_text(&idx.name)
					.with_text(&button_text)
					.with_state(
						match (ws.focused, ws.urgent) {
							(false, false) => State::Idle,
							(false, true ) => State::Critical,
							(true,  false) => State::Good,
							(true,  true ) => State::Warning,
						}
					)
			);
		}
        Ok(Some(self.update_interval.into()))
    }
	//}}}

	//{{{
    fn view(&self) -> Vec<&dyn I3BarWidget> {
		let mut elements: Vec<&dyn I3BarWidget> = Vec::new();

		for button in &self.ws_buttons {
			elements.push(button);
		}

		elements
    }
	//}}}

	//{{{
    fn click(&mut self, event: &I3BarEvent) -> Result<()> {
        if let Some(ref name) = event.name {
			for ws_button in &mut self.ws_buttons
			{
				if name == ws_button.id()
				{
					// The workspace name is encoded into the button id. So let's extract it
					let ws_name = ws_button.id().strip_prefix( &format!("{}_", self.id) ).unwrap_or_else(|| &ws_button.id()).to_string();

					if let Err(e) = self.sway_connection.run_command(format!("workspace {}", ws_name))
					{
						ws_button.set_text("error");
						// TODO: Is there a better way to return this error?
						return Err(BlockError("workspaces".to_string(), format!("workspaces::click(): cannot switch workspace: {}", e)))
					}
					break;
				}
			}
		}
        Ok(())
    }
	//}}}

	//{{{
    fn id(&self) -> &str {
        &self.id
    }
	//}}}
}
//}}}
