use std::sync::{Arc, Mutex};

pub struct Logger {
    logs: Arc<Mutex<Vec<String>>>,
}

impl Logger {
    pub fn new() -> Self {
        Self { logs: Arc::new(Mutex::new(Vec::new())) }
    }

    pub fn info(&mut self, message: &str) {
        self.logs.lock().unwrap().push(format!("[Info] {}", message.to_string()));
    }

    pub fn error(&mut self, message: &str) {
        self.logs.lock().unwrap().push(format!("[Error] {}", message.to_string()));
    }

    pub fn warn(&mut self, message: &str) {
        self.logs.lock().unwrap().push(format!("[Warn] {}", message.to_string()));
    }

    pub fn dump(&self) -> Vec<String> {
        self.logs.lock().unwrap().clone()
    }

    pub fn clone(&self) -> Self {
        Self { logs: self.logs.clone() }
    }
}
