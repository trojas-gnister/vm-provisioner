use log::{Log, Metadata, Record};
use std::sync::{Arc, Mutex};

pub struct TuiLogger {
    buffer: Arc<Mutex<Vec<String>>>,
}

impl Log for TuiLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let line = format!("[{}] {}", record.level(), record.args());
            if let Ok(mut buf) = self.buffer.lock() {
                buf.push(line);
            }
        }
    }

    fn flush(&self) {}
}

/// Install a TUI logger as the global logger and return the shared log buffer.
pub fn init() -> Arc<Mutex<Vec<String>>> {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let logger = TuiLogger {
        buffer: Arc::clone(&buffer),
    };
    // Ignore error if logger is already set (e.g. in tests)
    let _ = log::set_boxed_logger(Box::new(logger));
    log::set_max_level(log::LevelFilter::Info);
    buffer
}
