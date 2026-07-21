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
                // 持锁覆盖 write_all + flush，避免多线程下 shader 源码 dump
                // 与单行日志交错的缓冲区串写问题（LineWriter 缓冲残留被其他线程触发写出）。
                let stderr = std::io::stderr();
                let mut handle = stderr.lock();
                let _ = handle
                    .write_all(format!("[{}] {}\n", record.level(), record.args()).as_bytes());
                let _ = handle.flush();
            }
        }

        fn flush(&self) {}
    }

    static LOGGER: OnceLock<Logger> = OnceLock::new();
    let logger = LOGGER.get_or_init(|| Logger { max_level });
    // set_logger 可能因已有 logger 而失败，但 set_max_level 是全局的，必须单独调用
    let _ = log::set_logger(logger);
    log::set_max_level(max_level);
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
