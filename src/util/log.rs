use crate::config::{Config, LogLevel};
use log::LevelFilter;

/// Initialise the global logger using settings from `Config`.
///
/// On Android this uses `android_logger`; on other platforms it installs a
/// small stderr logger that respects the `FLUORATEGL_LOG` level.
pub fn init(cfg: &Config) {
    let max_level = level_filter(cfg.log_level);

    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default().with_max_level(max_level),
        );
    }

    #[cfg(not(target_os = "android"))]
    {
        use std::sync::OnceLock;

        struct Logger {
            max_level: LevelFilter,
        }

        impl log::Log for Logger {
            fn enabled(&self, metadata: &log::Metadata) -> bool {
                metadata.level() <= self.max_level
            }

            fn log(&self, record: &log::Record) {
                if self.enabled(record.metadata()) {
                    eprintln!("[{}] {}", record.level(), record.args());
                }
            }

            fn flush(&self) {}
        }

        static LOGGER: OnceLock<Logger> = OnceLock::new();
        let logger = LOGGER.get_or_init(|| Logger { max_level });
        let _ = log::set_logger(logger).map(|()| log::set_max_level(max_level));
    }
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
