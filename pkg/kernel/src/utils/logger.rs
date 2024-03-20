use log::{Metadata, Record};

pub fn init(log_level: &str) {
    static LOGGER: Logger = Logger;
    log::set_logger(&LOGGER).unwrap();

    match log_level {
        "Error" => log::set_max_level(log::LevelFilter::Error),
        "Warn" => log::set_max_level(log::LevelFilter::Warn),
        "Info" => log::set_max_level(log::LevelFilter::Info),
        "Debug" => log::set_max_level(log::LevelFilter::Debug),
        "Trace" => log::set_max_level(log::LevelFilter::Trace),
        _ => log::set_max_level(log::LevelFilter::Info),
    }
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
