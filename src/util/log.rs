use crate::config::{Config, LogLevel};
use log::LevelFilter;

/// Initialise the global logger using settings from `Config`.
///
/// Uses a simple stderr-based logger on all platforms so that log output
/// consistently appears alongside `eprintln!` messages (e.g. dispatch warnings)
/// under the same logcat tag on Android.
pub fn init(cfg: &Config) {
    use std::sync::OnceLock;

    let max_level = level_filter(cfg.log_level);

    struct Logger {
        max_level: LevelFilter,
    }

    impl log::Log for Logger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= self.max_level
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                use std::io::Write;
                let _ = std::io::stderr().write_all(
                    format!("[{}] {}\n", record.level(), record.args()).as_bytes(),
                );
                let _ = std::io::stderr().flush();
            }
        }

        fn flush(&self) {}
    }

    static LOGGER: OnceLock<Logger> = OnceLock::new();
    let logger = LOGGER.get_or_init(|| Logger { max_level });
    let _ = log::set_logger(logger).map(|()| log::set_max_level(max_level));
}

fn level_filter(level: LogLevel) -> LevelFilter {
    match level {
        LogLevel::Error => LevelFilter::Error,
        LogLevel::Warn => LevelFilter::Warn,
        LogLevel::Info => LevelFilter::Info,
        LogLevel::Debug => LevelFilter::Debug,
        LogLevel::Trace => LevelFilter::Trace,
    }
}
