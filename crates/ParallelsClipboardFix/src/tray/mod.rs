#[cfg(windows)]
mod win32;

#[cfg(not(windows))]
mod win32;

use std::fmt::{Display, Formatter};
use std::sync::mpsc::Sender;

#[derive(Debug)]
pub struct TrayController {
    inner: win32::TrayHandle,
}

impl TrayController {
    pub fn start(sender: Sender<TrayCommand>, state: TrayState) -> Result<Self, TrayError> {
        let inner = win32::start_tray(sender, state)?;
        Ok(Self { inner })
    }

    pub fn is_running(&self) -> bool {
        self.inner.is_running()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayState {
    pub enabled: bool,
    pub start_with_windows: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    FixNow,
    ToggleEnabled,
    ToggleStartup,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItemId {
    FixNow = 1001,
    ToggleEnabled = 1002,
    ToggleStartup = 1003,
    Quit = 1004,
}

impl MenuItemId {
    pub fn command(self) -> TrayCommand {
        match self {
            Self::FixNow => TrayCommand::FixNow,
            Self::ToggleEnabled => TrayCommand::ToggleEnabled,
            Self::ToggleStartup => TrayCommand::ToggleStartup,
            Self::Quit => TrayCommand::Quit,
        }
    }

    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            1001 => Some(Self::FixNow),
            1002 => Some(Self::ToggleEnabled),
            1003 => Some(Self::ToggleStartup),
            1004 => Some(Self::Quit),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum TrayError {
    UnsupportedPlatform,
    WindowThreadFailed,
    WindowCreationFailed,
    IconCreationFailed,
}

impl Display for TrayError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => formatter.write_str("system tray is Windows-only"),
            Self::WindowThreadFailed => formatter.write_str("tray window thread failed"),
            Self::WindowCreationFailed => formatter.write_str("tray window creation failed"),
            Self::IconCreationFailed => formatter.write_str("tray icon creation failed"),
        }
    }
}

impl std::error::Error for TrayError {}

#[cfg(test)]
mod tests {
    use super::{MenuItemId, TrayCommand};

    #[test]
    fn maps_menu_ids_to_commands() {
        assert_eq!(
            MenuItemId::from_u16(1001).map(MenuItemId::command),
            Some(TrayCommand::FixNow)
        );
        assert_eq!(
            MenuItemId::from_u16(1004).map(MenuItemId::command),
            Some(TrayCommand::Quit)
        );
        assert_eq!(MenuItemId::from_u16(9999), None);
    }
}
