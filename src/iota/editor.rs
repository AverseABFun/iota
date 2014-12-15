extern crate rustbox;

use std::comm::{Receiver, Sender};
use std::char;
use std::io::{fs, File, FileMode, FileAccess, TempDir};
use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicBool};

use super::Response;
use input::Input;
use cursor::Direction;
use keyboard::Key;
use view::View;


enum EventStatus {
    Handled(Response),
    NotHandled,
}


pub struct Editor<'e> {
    pub running: Arc<AtomicBool>,
    pub sender: Sender<rustbox::Event>,

    events: Receiver<rustbox::Event>,
    view: View<'e>,
}

impl<'e> Editor<'e> {
    pub fn new(source: Input) -> Editor<'e> {
        let view = View::new(source);

        let (send, recv) = channel();
        Editor {
            sender: send,
            events: recv,
            view: view,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn handle_key_event(&mut self, key: u16, ch: u32) -> Response {
        let key = match key {
            0 => char::from_u32(ch).map(|c| Key::Char(c)),
            a => Key::from_special_code(a),
        };

        match self.handle_system_event(key) {
            EventStatus::Handled(response) => { response }
            EventStatus::NotHandled        => { Response::Continue }
        }
    }

    pub fn save_active_buffer(&mut self) {
        let lines = &self.view.buffer.lines;
        let path = self.view.buffer.file_path.as_ref().unwrap();

        let tmpdir = match TempDir::new_in(&Path::new("."), "iota") {
            Ok(d) => d,
            Err(e) => panic!("file error: {}", e)
        };

        let tmppath = tmpdir.path().join(Path::new("tmpfile"));

        let mut file = match File::open_mode(&tmppath, FileMode::Open, FileAccess::Write) {
            Ok(f) => f,
            Err(e) => panic!("file error: {}", e)
        };

        for line in lines.iter() {
            let mut data = line.data.clone();
            data.push('\n');
            let result = file.write(data.as_bytes());

            if result.is_err() {
                // TODO(greg): figure out what to do here.
                panic!("Something went wrong while writing the file");
            }
        }

        if let Err(e) = fs::rename(&tmppath, path) {
            panic!("file error: {}", e);
        }
    }

    pub fn draw(&mut self) {
        self.view.draw();
        self.view.draw_status();
        self.view.draw_cursor();
    }

    pub fn start(&mut self) {
        // Synchronizes with transfer across thread boundary
        self.running.store(true, Ordering::Relaxed);
        self.event_loop();
        self.main_loop();
    }

    fn main_loop(&mut self) {
        while self.running.load(Ordering::Relaxed) {
            self.view.clear();
            self.draw();
            rustbox::present();
            if let rustbox::Event::KeyEvent(_, key, ch) = self.events.recv() {
                if let Response::Quit = self.handle_key_event(key, ch) {
                    // Okay if it doesn't quit immediately.
                    self.running.store(false, Ordering::Relaxed);
                }
            }
        }
    }

    fn event_loop(&self) {
        // clone the sender so that we can use it in the proc
        let sender = self.sender.clone();
        let running = self.running.clone();

        spawn(proc() {
            while running.load(Ordering::Relaxed) {
                if sender.send_opt(rustbox::peek_event(1000)).is_err() {
                    running.store(false, Ordering::Relaxed);
                }
            }
        });
    }

    fn handle_system_event(&mut self, k: Option<Key>) -> EventStatus {
        let key = match k {
            Some(k) => k,
            None => return EventStatus::NotHandled
        };

        match key {
            Key::Up        => { self.view.move_cursor(Direction::Up); }
            Key::Down      => { self.view.move_cursor(Direction::Down); }
            Key::Left      => { self.view.move_cursor(Direction::Left); }
            Key::Right     => { self.view.move_cursor(Direction::Right); }
            Key::Enter     => { self.view.insert_line(); }

            // Tab inserts 4 spaces, rather than a \t
            Key::Tab       => { self.view.insert_tab(); }

            Key::Backspace => { self.view.delete_char(Direction::Left); }
            Key::Delete    => { self.view.delete_char(Direction::Right); }
            Key::CtrlS     => { self.save_active_buffer(); }
            Key::CtrlQ     => { return EventStatus::Handled(Response::Quit) }
            Key::CtrlR     => { self.view.resize(); }

            Key::Char(c)   => { self.view.insert_char(c) }

            // default
            _              => { return EventStatus::NotHandled }
        }
        // event is handled and we want to keep the editor running
        EventStatus::Handled(Response::Continue)
    }

}
