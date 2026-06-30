use std::env;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub enabled: bool,
    pub start_with_windows: bool,
    pub retry_count: u32,
    pub retry_delay_ms: u64,
    pub write_png: bool,
    pub write_dib: bool,
    pub log_level: LogLevel,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            start_with_windows: false,
            retry_count: 5,
            retry_delay_ms: 80,
            write_png: true,
            write_dib: true,
            log_level: LogLevel::Info,
        }
    }
}

impl AppConfig {
    pub fn load_or_create() -> Result<Self, ConfigError> {
        let path = default_config_path()?;
        Self::load_or_create_at(path)
    }

    pub fn load_or_create_at(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        match fs::read_to_string(path) {
            Ok(contents) => Self::parse(&contents),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let config = Self::default();
                config.save_to(path)?;
                Ok(config)
            }
            Err(error) => Err(ConfigError::Io(error)),
        }
    }

    pub fn save_to(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.to_toml())?;
        Ok(())
    }

    pub fn parse(input: &str) -> Result<Self, ConfigError> {
        let mut config = Self::default();

        for (index, raw_line) in input.lines().enumerate() {
            let line_number = index + 1;
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                return Err(ConfigError::Parse {
                    line: line_number,
                    message: "expected key = value".to_owned(),
                });
            };

            let key = key.trim();
            let value = value.trim();

            match key {
                "enabled" => config.enabled = parse_bool(value, line_number)?,
                "start_with_windows" => {
                    config.start_with_windows = parse_bool(value, line_number)?;
                }
                "retry_count" => config.retry_count = parse_u32(value, line_number)?,
                "retry_delay_ms" => config.retry_delay_ms = parse_u64(value, line_number)?,
                "write_png" => config.write_png = parse_bool(value, line_number)?,
                "write_dib" => config.write_dib = parse_bool(value, line_number)?,
                "log_level" => config.log_level = LogLevel::parse(value, line_number)?,
                _ => {
                    return Err(ConfigError::Parse {
                        line: line_number,
                        message: format!("unknown key `{key}`"),
                    });
                }
            }
        }

        Ok(config)
    }

    pub fn to_toml(&self) -> String {
        format!(
            concat!(
                "enabled = {}\n",
                "start_with_windows = {}\n",
                "retry_count = {}\n",
                "retry_delay_ms = {}\n",
                "write_png = {}\n",
                "write_dib = {}\n",
                "log_level = \"{}\"\n",
            ),
            self.enabled,
            self.start_with_windows,
            self.retry_count,
            self.retry_delay_ms,
            self.write_png,
            self.write_dib,
            self.log_level
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn parse(value: &str, line: usize) -> Result<Self, ConfigError> {
        let normalized = unquote(value).to_ascii_lowercase();
        match normalized.as_str() {
            "error" => Ok(Self::Error),
            "warn" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            _ => Err(ConfigError::Parse {
                line,
                message: format!("invalid log level `{value}`"),
            }),
        }
    }
}

impl Display for LogLevel {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => formatter.write_str("error"),
            Self::Warn => formatter.write_str("warn"),
            Self::Info => formatter.write_str("info"),
            Self::Debug => formatter.write_str("debug"),
            Self::Trace => formatter.write_str("trace"),
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    MissingConfigDirectory,
    Parse { line: usize, message: String },
}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "config I/O error: {error}"),
            Self::MissingConfigDirectory => formatter.write_str("could not find config directory"),
            Self::Parse { line, message } => {
                write!(formatter, "config parse error on line {line}: {message}")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::MissingConfigDirectory | Self::Parse { .. } => None,
        }
    }
}

impl From<io::Error> for ConfigError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn default_config_path() -> Result<PathBuf, ConfigError> {
    config_dir().map(|directory| directory.join(CONFIG_FILE_NAME))
}

pub fn config_dir() -> Result<PathBuf, ConfigError> {
    let base = env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or(ConfigError::MissingConfigDirectory)?;

    Ok(base.join("ParallelsClipboardFix"))
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(before, _)| before)
}

fn parse_bool(value: &str, line: usize) -> Result<bool, ConfigError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ConfigError::Parse {
            line,
            message: format!("expected boolean, got `{value}`"),
        }),
    }
}

fn parse_u32(value: &str, line: usize) -> Result<u32, ConfigError> {
    value.parse().map_err(|_| ConfigError::Parse {
        line,
        message: format!("expected unsigned integer, got `{value}`"),
    })
}

fn parse_u64(value: &str, line: usize) -> Result<u64, ConfigError> {
    value.parse().map_err(|_| ConfigError::Parse {
        line,
        message: format!("expected unsigned integer, got `{value}`"),
    })
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|without_prefix| without_prefix.strip_suffix('"'))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, LogLevel};

    #[test]
    fn parses_supported_keys() {
        let input = r#"
            enabled = false
            start_with_windows = true
            retry_count = 7
            retry_delay_ms = 125
            write_png = false
            write_dib = true
            log_level = "debug"
        "#;

        let config = AppConfig::parse(input).unwrap_or_else(|error| panic!("{error}"));

        assert!(!config.enabled);
        assert!(config.start_with_windows);
        assert_eq!(config.retry_count, 7);
        assert_eq!(config.retry_delay_ms, 125);
        assert!(!config.write_png);
        assert!(config.write_dib);
        assert_eq!(config.log_level, LogLevel::Debug);
    }

    #[test]
    fn serializes_default_config() {
        let toml = AppConfig::default().to_toml();

        assert!(toml.contains("enabled = true"));
        assert!(toml.contains("log_level = \"info\""));
    }
}
