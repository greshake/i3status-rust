use std::collections::BTreeMap;
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
use swayipc::{Connection, EventType, Event::Mode};
use std::sync::{Arc, Mutex};
use crate::widgets::button::ButtonWidget;

//{{{
pub struct Bindingmode {
	id: String,

	sway_connection: swayipc::Connection,

	active_mode:  Arc<Mutex<String>>,
	mode_button: Option<ButtonWidget>,
	show_default_mode: bool,

	config: Config,
}
//}}}

//{{{ pub struct BindingmodeConfig {

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct BindingmodeConfig {
	#[serde(default = "BindingmodeConfig::default_show_default_mode")]
	pub show_default_mode: bool,

	#[serde(default = "BindingmodeConfig::default_color_overrides")]
	pub color_overrides: Option<BTreeMap<String, String>>,
}
//}}}

//{{{
impl BindingmodeConfig {
	fn default_show_default_mode() -> bool {
		false
	}

	fn default_color_overrides() -> Option<BTreeMap<String, String>> {
		None
	}
}
//}}}

//{{{
impl ConfigBlock for Bindingmode {
	type Config = BindingmodeConfig;

	//{{{
	fn new( block_config: Self::Config, config: Config, tx_update_request: Sender<Task>,) -> Result<Self>
	{
		let id: String = pseudo_uuid();
		let id_clone = id.clone();

		let active_mode_original = Arc::new(Mutex::new(String::new()));
		let active_mode = active_mode_original.clone();

		let mode_button = None;

		let mut sway_connection = Connection::new().block_error("workspaces", "failed to acquire connect to IPC")?;

		//{{{ Add initial workspaces
		{
			let mut active_mode = active_mode_original.lock().unwrap();
			*active_mode = sway_connection.get_binding_state().unwrap();
		}
		//}}}

		//{{{
		thread::Builder::new()
			.name("bindingmode".into())
			.spawn(move || {
				for event in Connection::new()
					.unwrap()
					.subscribe(&[EventType::Mode])
					.unwrap()
				{
					match event.unwrap() {
						Mode(e) => {
						//ModeEvent { change, pango_markup, .. } => {
							let mut active_mode = active_mode_original.lock().unwrap();
							*active_mode = e.change;

							tx_update_request.send(Task {
								id: id_clone.clone(),
								update_time: Instant::now(),
							})
							.unwrap();
						}
						_ => unreachable!(),
						//_ => {},
					}
				}
			})
			.unwrap();
		//}}}

		Ok(Bindingmode {
			id: id.clone(),
			show_default_mode: block_config.show_default_mode,

			sway_connection: sway_connection,
			config: config,
			active_mode: active_mode,
			mode_button: mode_button,
		})
	}
	//}}}
}
//}}}

//{{{
impl Block for Bindingmode {
	//{{{
	fn update(&mut self) -> Result<Option<Update>> {
		let active_mode = &(*self.active_mode.lock().block_error("bindingmode", "failed to aquire lock")?);
		self.mode_button = if active_mode != "default" || self.show_default_mode
		{
			Some(
				ButtonWidget::new(self.config.clone(), &self.id.clone())
					.with_text(&active_mode)
					.with_state(if active_mode == "default" {State::Idle} else {State::Critical})
			)
		} else { None };
		Ok(None)
	}
	//}}}

	//{{{
	fn view(&self) -> Vec<&dyn I3BarWidget> {
		match &self.mode_button {
			Some(but) => vec![but],
			None      => vec![],
		}
	}
	//}}}

	//{{{
	fn click(&mut self, event: &I3BarEvent) -> Result<()> {
		if let Some(ref name) = event.name {
			if name.contains(&self.id) { // save some performance: only check each button if it was the workspaces block that was clicked...
				if let Some(btn) = &mut self.mode_button
				{
					if let Err(e) = self.sway_connection.run_command("mode default".to_string())
					{
						btn.set_text("error");
						// TODO: Is there a better way to return this error?
						return Err(BlockError("workspaces".to_string(), format!("workspaces::click(): cannot switch workspace: {}", e)))
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
