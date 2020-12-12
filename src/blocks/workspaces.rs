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
	name:    String,
}

//{{{
impl Ord for WsKey {
    fn cmp(&self, other: &Self) -> Ordering {
		if let Some(s) = self.num
		{
			if let Some(o) = other.num
			{
				s.cmp(&o)
			}
			else
			{
				Ordering::Less
			}
		}
		else
		{
			if let Some(_) = other.num
			{
				Ordering::Greater
			}
			else
			{
				self.name.cmp(&other.name)
			}
		}
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
	//id:      i64,
	//name:    String,
	urgent:  bool,
	focused: bool,
	config: Config,
}
//}}}

//{{{
pub struct Workspaces {
    id: String,
    update_interval: Duration,

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

        let _test_conn = Connection::new().block_error("workspaces", "failed to acquire connect to IPC")?;

		//{{{
		for workspace in Connection::new().unwrap().get_workspaces().unwrap()
		{
			let mut workspaces = workspaces_original.lock().unwrap();
			workspaces.insert(
				WsKey {
				           num:  Some(workspace.num),
				           name: workspace.name
				},
				WS    {
				           /*id: ws_current.id,*/
				           urgent: workspace.urgent,
				           focused: workspace.focused,
				           config: config_clone.clone()
				}
			);
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

										let ws_name = if let Some(name) = ws_current.name { name } else { String::from("") };

										workspaces.insert(
											WsKey {
												num: ws_current.num,
												name: ws_name,
											},
														  WS {//id: ws_current.id,
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

										if let Some(ws_name) = ws_current.name
										{
											//workspaces.remove(&ws_name);
											workspaces.remove(&WsKey{ num: ws_current.num, name: ws_name});
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
								WorkspaceChange::Focus  => {
									let mut workspaces = workspaces_original.lock().unwrap();

                                    if let Some(ws_current) = e.current {
										if let Some(ws_name) = ws_current.name
										{
											if let Some(ws) = workspaces.get_mut(&WsKey{num: ws_current.num, name: ws_name}) {
												ws.focused = true;
											}
										}
									}

                                    if let Some(ws_old) = e.old {
										if let Some(ws_name) = ws_old.name
										{
											if let Some(ws) = workspaces.get_mut(&WsKey{ num: ws_old.num, name: ws_name}) {
												ws.focused = false;
											}
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
            config,
			workspaces,
			ws_buttons,
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
			self.ws_buttons.push_back(
				ButtonWidget::new(ws.config.clone(), &idx.name)
					.with_text(&idx.name)
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
    fn click(&mut self, _: &I3BarEvent) -> Result<()> {
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
