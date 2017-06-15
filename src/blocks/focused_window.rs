use std::time::{Instant, Duration};
use std::sync::mpsc::Sender;
use std::thread;
use std::sync::{Arc, Mutex};

use block::{Block, ConfigBlock};
use config::Config;
use widgets::text::TextWidget;
use widget::I3BarWidget;
use input::I3BarEvent;
use scheduler::Task;

use uuid::Uuid;

extern crate i3ipc;
use self::i3ipc::I3EventListener;
use self::i3ipc::Subscription;
use self::i3ipc::event::Event;
use self::i3ipc::event::inner::WindowChange;


pub struct FocusedWindow {
    text: TextWidget,
    title: Arc<Mutex<String>>,
    max_width: usize,
    id: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct FocusedWindowConfig {
    /// Truncates titles if longer than max-width
    #[serde(default = "FocusedWindowConfig::default_max_width")]
    pub max_width: usize,
}

impl FocusedWindowConfig {
    fn default_max_width() -> usize {
        21
    }
}


impl ConfigBlock for FocusedWindow {
    type Config = FocusedWindowConfig;

    fn new(block_config: Self::Config, config: Config, tx: Sender<Task>) -> Self {
        let id = Uuid::new_v4().simple().to_string();
        let id_clone = id.clone();

        let title_original = Arc::new(Mutex::new(String::from("")));
        let title = title_original.clone();

        thread::spawn(move || {
            // establish connection.
            let mut listener = I3EventListener::connect().unwrap();

            // subscribe to a couple events.
            let subs = [Subscription::Window];
            listener.subscribe(&subs).unwrap();

            // handle them
            for event in listener.listen() {
                match event.unwrap() {
                    Event::WindowEvent(e) => {
                        match e.change {
                            WindowChange::Focus => {
                                    if let Some(name) = e.container.name {
                                        let mut title = title_original.lock().unwrap();
                                        *title = name;
                                        tx.send(Task {id: id_clone.clone(), update_time: Instant::now()}).unwrap();
                                    }
                                },
                            WindowChange::Title => {
                                if e.container.focused {
                                    if let Some(name) = e.container.name {
                                        let mut title = title_original.lock().unwrap();
                                        *title = name;
                                        tx.send(Task {id: id_clone.clone(), update_time: Instant::now()}).unwrap();
                                    }
                                }
                            }
                            _ => {}
                        };
                    }
                    _ => unreachable!()
                }
            }
        });

        FocusedWindow {
            id,
            text: TextWidget::new(config),
            max_width: block_config.max_width,
            title
        }
    }
}


impl Block for FocusedWindow
{
    fn update(&mut self) -> Option<Duration> {
        let mut string = (*self.title.lock().unwrap()).clone();
        string.truncate(self.max_width);
        self.text.set_text(string);
        None
    }
    fn view(&self) -> Vec<&I3BarWidget> {
        vec![&self.text]
    }
    fn click(&mut self, _: &I3BarEvent) {}
    fn id(&self) -> &str {
        &self.id
    }
}
