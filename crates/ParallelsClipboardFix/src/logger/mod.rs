use std::fmt::{Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{self, LogLevel};

const LOG_FILE_NAME: &str = "ParallelsClipboardFix.log";

#[derive(Clone)]
pub struct Logger {
    level: LogLevel,
    writer: Arc<Mutex<File>>,
}

impl Logger {
    pub fn open(level: LogLevel) -> Result<Self, LoggerError> {
        let path = default_log_path()?;
        Self::open_at(path, level)
    }

    pub fn open_at(path: impl AsRef<Path>, level: LogLevel) -> Result<Self, LoggerError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new().create(true).append(true).open(path)?;

        Ok(Self {
            level,
            writer: Arc::new(Mutex::new(file)),
        })
    }

    pub fn error(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Error, message.as_ref());
    }

    pub fn warn(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Warn, message.as_ref());
    }

    pub fn info(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Info, message.as_ref());
    }

    pub fn debug(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Debug, message.as_ref());
    }

    pub fn trace(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Trace, message.as_ref());
    }

    pub fn log(&self, level: LogLevel, message: &str) {
        if !level_enabled(self.level, level) {
            return;
        }

        let Ok(mut writer) = self.writer.lock() else {
            return;
        };

        let _ = writeln!(
            writer,
            "{} [{level}] {}",
            unix_timestamp_seconds(),
            sanitize_message(message)
        );
    }
}

#[derive(Debug)]
pub enum LoggerError {
    Io(io::Error),
    Config(config::ConfigError),
}

impl Display for LoggerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "logger I/O error: {error}"),
            Self::Config(error) => write!(formatter, "logger path error: {error}"),
        }
    }
}

impl std::error::Error for LoggerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Config(error) => Some(error),
        }
    }
}

impl From<io::Error> for LoggerError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<config::ConfigError> for LoggerError {
    fn from(error: config::ConfigError) -> Self {
        Self::Config(error)
    }
}

pub fn default_log_path() -> Result<PathBuf, LoggerError> {
    Ok(config::config_dir()?.join(LOG_FILE_NAME))
}

fn level_enabled(configured: LogLevel, event: LogLevel) -> bool {
    level_rank(event) <= level_rank(configured)
}

fn level_rank(level: LogLevel) -> u8 {
    match level {
        LogLevel::Error => 1,
        LogLevel::Warn => 2,
        LogLevel::Info => 3,
        LogLevel::Debug => 4,
        LogLevel::Trace => 5,
    }
}

fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn sanitize_message(message: &str) -> String {
    message.replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::config::LogLevel;

    use super::{sanitize_message, Logger};

    #[test]
    fn writes_enabled_levels() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join("ParallelsClipboardFix-logger-test.log");
        let _ = fs::remove_file(&path);

        let logger = Logger::open_at(&path, LogLevel::Info)?;
        logger.debug("hidden");
        logger.info("visible");

        let contents = fs::read_to_string(&path)?;
        assert!(contents.contains("[info] visible"));
        assert!(!contents.contains("hidden"));

        let _ = fs::remove_file(&path);
        Ok(())
    }

    #[test]
    fn sanitizes_multiline_messages() {
        assert_eq!(sanitize_message("a\nb\rc"), "a b c");
    }
}
