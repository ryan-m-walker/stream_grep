use std::sync::{Arc, Mutex};

pub struct Logger {
    logs: Arc<Mutex<Vec<String>>>,
}

impl Logger {
    pub fn new() -> Self {
        Self { logs: Arc::new(Mutex::new(Vec::new())) }
    }

    pub fn info(&mut self, message: &str) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.push(format!("[Info] {}", message));
        }
    }

    pub fn error(&mut self, message: &str) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.push(format!("[Error] {}", message));
        }
    }

    pub fn warn(&mut self, message: &str) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.push(format!("[Warn] {}", message));
        }
    }

    pub fn dump(&self) {
        if let Ok(logs) = self.logs.lock() {
            println!("\n--- DEV LOGS ---");
            for line in logs.iter() {
                println!("{}", line);
            }
            println!("----------------\n");
        }
    }

    pub fn clone(&self) -> Self {
        Self { logs: self.logs.clone() }
    }
}
