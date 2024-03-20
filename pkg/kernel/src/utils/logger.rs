use log::{Metadata, Record};

pub fn init() {
    static LOGGER: Logger = Logger;
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    info!("Logger Initialized.");
}

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= metadata.level()
    }

    fn log(&self, record: &Record) {
        // Implement the logger with serial output
        if self.enabled(record.metadata()) {
            println!("[{}]: {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}
