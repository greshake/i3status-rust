use std::time::{Instant, Duration};
use std::sync::mpsc::Sender;
use std::thread;
use std::sync::{Arc, Mutex};

use block::Block;
use widgets::text::TextWidget;
use widget::I3BarWidget;
use input::I3barEvent;
use scheduler::Task;

use serde_json::Value;
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

impl FocusedWindow {
    pub fn new(config: Value, tx: Sender<Task>, theme: Value) -> FocusedWindow {
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

        {
            FocusedWindow {
                id,
                text: TextWidget::new(theme.clone()),
                max_width: get_u64_default!(config, "max-width", 21) as usize,
                title
            }
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
    fn click_left(&mut self, _: &I3barEvent) {}
    fn id(&self) -> &str {
        &self.id
    }
}
