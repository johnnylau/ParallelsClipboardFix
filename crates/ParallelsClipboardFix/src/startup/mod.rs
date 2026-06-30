#[cfg(windows)]
mod win32;

#[cfg(not(windows))]
mod win32;

use std::env;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};

const SHORTCUT_FILE_NAME: &str = "ParallelsClipboardFix.lnk";

#[derive(Debug, Clone)]
pub struct StartupShortcut {
    path: PathBuf,
}

impl StartupShortcut {
    pub fn new() -> Result<Self, StartupError> {
        Ok(Self {
            path: startup_shortcut_path()?,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_installed(&self) -> bool {
        self.path.exists()
    }

    pub fn install(&self, executable: impl AsRef<Path>) -> Result<(), StartupError> {
        win32::create_shortcut(executable.as_ref(), &self.path)
    }

    pub fn uninstall(&self) -> Result<(), StartupError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(StartupError::Io(error)),
        }
    }
}

#[derive(Debug)]
pub enum StartupError {
    UnsupportedPlatform,
    MissingStartupDirectory,
    InvalidPath,
    Io(io::Error),
    ComFailed(&'static str),
}

impl Display for StartupError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => formatter.write_str("startup shortcuts are Windows-only"),
            Self::MissingStartupDirectory => {
                formatter.write_str("could not find the user Startup folder")
            }
            Self::InvalidPath => formatter.write_str("path cannot be represented for Windows APIs"),
            Self::Io(error) => write!(formatter, "startup shortcut I/O error: {error}"),
            Self::ComFailed(operation) => {
                write!(formatter, "startup shortcut COM call failed: {operation}")
            }
        }
    }
}

impl std::error::Error for StartupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::UnsupportedPlatform
            | Self::MissingStartupDirectory
            | Self::InvalidPath
            | Self::ComFailed(_) => None,
        }
    }
}

impl From<io::Error> for StartupError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn startup_shortcut_path() -> Result<PathBuf, StartupError> {
    startup_directory().map(|directory| directory.join(SHORTCUT_FILE_NAME))
}

fn startup_directory() -> Result<PathBuf, StartupError> {
    let appdata = env::var_os("APPDATA").ok_or(StartupError::MissingStartupDirectory)?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup"))
}

#[cfg(test)]
mod tests {
    use super::SHORTCUT_FILE_NAME;

    #[test]
    fn shortcut_name_is_lnk() {
        assert_eq!(SHORTCUT_FILE_NAME, "ParallelsClipboardFix.lnk");
    }
}
