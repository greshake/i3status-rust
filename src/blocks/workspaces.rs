use std::collections::{LinkedList, BTreeMap};
use std::time::Instant;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::Config;
use crate::errors::*;
use crate::input::I3BarEvent;
use crate::scheduler::Task;
use crate::util::pseudo_uuid;
use crate::widget::{I3BarWidget, State};

use std::thread;
use swayipc::{Connection, EventType, WorkspaceChange, Event::Workspace};
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
	fn new(num_opt: Option<i32>, name_opt: Option<String>) -> WsKey
	{
		match (num_opt, name_opt) {
			(None,      None)                                  => unreachable!(),
			(None,      Some(name))                            => WsKey{ num:  None,      name: Some(name) },
			(Some(num), None)                                  => WsKey{ num:  Some(num), name: None },
			(Some(num), Some(name)) if num == -1               => WsKey{ num:  None,      name: Some(name) },
			(Some(num), Some(name)) if num.to_string() == name => WsKey{ num:  Some(num), name: None },
			(Some(num), Some(name))                            => WsKey{ num:  Some(num), name: Some(name) },
		}
	}
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
			(Some(s), Some(o)) => if s<o {-10} else if s==o {0} else {10},
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
		match (self.num, other.num, &self.name, &other.name)
		{
			(Some(s_num), Some(o_num), Some(s_name), Some(o_name)) => s_num==o_num && s_name==o_name,

			(Some(s_num), Some(o_num), None,         None)         => s_num==o_num,
			(None,        None,        Some(s_name), Some(o_name)) => s_name==o_name,

			(Some(_),     None,        _,            _)            => false,
			(None,        Some(_),     _,            _)            => false,

			(_,           _,           Some(_),      None)         => false,
			(_,           _,           None,         Some(_))      => false,
			(None,        None,        None,         None)         => unreachable!(),
		}
	}
}
//}}}
//}}}

//{{{
struct WS {
	urgent:  bool,
	focused: bool,
}
//}}}

//{{{
pub struct Workspaces {
	id: String,
	strip_workspace_numbers: bool,
	strip_workspace_name:    bool,

	sway_connection: swayipc::Connection,

	workspaces: Arc<Mutex<BTreeMap<WsKey, WS>>>,
	ws_buttons: LinkedList<ButtonWidget>,

	config: Config,
}
//}}}

//{{{ pub struct WorkspacesConfig {

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct WorkspacesConfig {
	#[serde(default = "WorkspacesConfig::default_strip_workspace_numbers")]
	pub strip_workspace_numbers: bool,

	#[serde(default = "WorkspacesConfig::default_strip_workspace_name")]
	pub strip_workspace_name: bool,

	#[serde(default = "WorkspacesConfig::default_color_overrides")]
	pub color_overrides: Option<BTreeMap<String, String>>,
}
//}}}

//{{{
impl WorkspacesConfig {
	fn default_strip_workspace_numbers() -> bool {
		false
	}

	fn default_strip_workspace_name() -> bool {
		false
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
						Workspace(e) => {
						//WorkspaceEvent { change, current, old, .. } => {
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
									let mut workspaces = workspaces_original.lock().unwrap();
									if let Some(ws_current) = e.current {
										if let Some(ws) = workspaces.get_mut(&WsKey::new(ws_current.num, ws_current.name)) {
											ws.urgent = ws_current.urgent;
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
								WorkspaceChange::Reload => {
								}
								//}}}
								_ => unreachable!(), // WorkspaceChange is marked Non-exhaustive
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
			strip_workspace_numbers: block_config.strip_workspace_numbers,
			strip_workspace_name:    block_config.strip_workspace_name,

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
			let button_text = match(idx.num, &idx.name, self.strip_workspace_numbers, self.strip_workspace_name) {
				//pathological cases first:
				(_,         _,          true,  true)  => "".to_string(),   // Well, what did you expect?
				(None,      None,       _,     _)     => "INVALID".to_string(),

				(None,      Some(name), _,     _)     => name.to_string(),
				(Some(num), None,       _,     _)     => num.to_string(),
				(Some(_),   Some(name), false, false) => name.to_string(), // counter-intuitively number is already part of the name...
				(Some(num), Some(_),    false, true)  => num.to_string(),
				(Some(num), Some(name), true,  false) => name.trim_start_matches(&num.to_string()).trim_start().to_string(),
			};
			let button_id = self.id.clone() + "_" +
				&(if let Some(name) = &idx.name { name.to_string() } else if let Some(num) = idx.num { num.to_string() } else { "INVALID".to_string() });

			self.ws_buttons.push_back(
				ButtonWidget::new(self.config.clone(), &button_id)
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
		Ok(None)
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
			if name.contains(&self.id) { // save some performance: only check each button if it was the workspaces block that was clicked...
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
