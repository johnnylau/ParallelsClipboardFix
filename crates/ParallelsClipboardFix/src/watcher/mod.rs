#[cfg(windows)]
mod win32;

#[cfg(not(windows))]
mod win32;

use std::fmt::{Display, Formatter};
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::config::AppConfig;

#[derive(Debug)]
pub struct ClipboardWatcher {
    inner: win32::WatcherHandle,
}

impl ClipboardWatcher {
    pub fn start(config: &AppConfig, sender: Sender<WatcherEvent>) -> Result<Self, WatcherError> {
        let retry_schedule = RetrySchedule::from_config(config);
        let inner = win32::start_watcher(sender, retry_schedule)?;
        Ok(Self { inner })
    }

    pub fn is_running(&self) -> bool {
        self.inner.is_running()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherEvent {
    ClipboardChanged,
    RetryRequested { attempt: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrySchedule {
    attempts: u32,
    delay: Duration,
}

impl RetrySchedule {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            attempts: config.retry_count,
            delay: Duration::from_millis(config.retry_delay_ms),
        }
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    pub fn delay(&self) -> Duration {
        self.delay
    }

    pub fn retry_events(&self) -> impl Iterator<Item = WatcherEvent> {
        (1..=self.attempts).map(|attempt| WatcherEvent::RetryRequested { attempt })
    }
}

#[derive(Debug)]
pub enum WatcherError {
    UnsupportedPlatform,
    WindowThreadFailed,
    WindowCreationFailed,
    ClipboardListenerFailed,
}

impl Display for WatcherError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => formatter.write_str("clipboard watcher is Windows-only"),
            Self::WindowThreadFailed => formatter.write_str("clipboard watcher thread failed"),
            Self::WindowCreationFailed => formatter.write_str("clipboard watcher window failed"),
            Self::ClipboardListenerFailed => {
                formatter.write_str("failed to register clipboard listener")
            }
        }
    }
}

impl std::error::Error for WatcherError {}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::config::AppConfig;

    use super::{RetrySchedule, WatcherEvent};

    #[test]
    fn builds_retry_schedule_from_config() {
        let config = AppConfig {
            retry_count: 3,
            retry_delay_ms: 25,
            ..AppConfig::default()
        };

        let schedule = RetrySchedule::from_config(&config);

        assert_eq!(schedule.attempts(), 3);
        assert_eq!(schedule.delay(), Duration::from_millis(25));
    }

    #[test]
    fn creates_retry_events() {
        let config = AppConfig {
            retry_count: 2,
            ..AppConfig::default()
        };
        let schedule = RetrySchedule::from_config(&config);
        let events: Vec<_> = schedule.retry_events().collect();

        assert_eq!(
            events,
            vec![
                WatcherEvent::RetryRequested { attempt: 1 },
                WatcherEvent::RetryRequested { attempt: 2 }
            ]
        );
    }
}
