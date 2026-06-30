#[cfg(windows)]
mod win32;

#[cfg(not(windows))]
mod win32;

use std::fmt::{Display, Formatter};
use std::time::{Duration, Instant};

use crate::config::AppConfig;

#[derive(Debug)]
pub struct ClipboardService {
    self_trigger_window: Duration,
    last_fix: Option<Instant>,
}

impl ClipboardService {
    pub fn new() -> Self {
        Self {
            self_trigger_window: Duration::from_millis(300),
            last_fix: None,
        }
    }

    pub fn fix_now(&mut self, config: &AppConfig) -> Result<FixOutcome, ClipboardError> {
        if !config.enabled {
            return Ok(FixOutcome::Disabled);
        }

        if self.is_probable_self_trigger() {
            return Ok(FixOutcome::SkippedSelfTrigger);
        }

        let outcome = win32::normalize_clipboard_image(config)?;
        if matches!(outcome, FixOutcome::Fixed { .. }) {
            self.last_fix = Some(Instant::now());
        }

        Ok(outcome)
    }

    fn is_probable_self_trigger(&self) -> bool {
        self.last_fix
            .map(|instant| instant.elapsed() < self.self_trigger_window)
            .unwrap_or(false)
    }
}

impl Default for ClipboardService {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixOutcome {
    Disabled,
    NoReadableImage,
    SkippedSelfTrigger,
    Fixed { wrote_dib: bool, wrote_png: bool },
}

#[derive(Debug)]
pub enum ClipboardError {
    UnsupportedPlatform,
    OpenClipboardFailed,
    EmptyClipboardFailed,
    ReadFormatFailed(&'static str),
    WriteFormatFailed(&'static str),
    InvalidImageData(&'static str),
    AllocationFailed,
}

impl Display for ClipboardError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => formatter.write_str("clipboard fixing is Windows-only"),
            Self::OpenClipboardFailed => formatter.write_str("failed to open clipboard"),
            Self::EmptyClipboardFailed => formatter.write_str("failed to empty clipboard"),
            Self::ReadFormatFailed(format) => write!(formatter, "failed to read {format}"),
            Self::WriteFormatFailed(format) => write!(formatter, "failed to write {format}"),
            Self::InvalidImageData(reason) => write!(formatter, "invalid image data: {reason}"),
            Self::AllocationFailed => formatter.write_str("failed to allocate clipboard memory"),
        }
    }
}

impl std::error::Error for ClipboardError {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClipboardImage {
    dib: Option<Vec<u8>>,
    png: Option<Vec<u8>>,
}

impl ClipboardImage {
    fn new(dib: Option<Vec<u8>>, png: Option<Vec<u8>>) -> Result<Self, ClipboardError> {
        if dib.is_none() && png.is_none() {
            return Err(ClipboardError::InvalidImageData("no image formats present"));
        }

        if let Some(bytes) = dib.as_deref() {
            validate_dib(bytes)?;
        }

        if let Some(bytes) = png.as_deref() {
            validate_png(bytes)?;
        }

        Ok(Self { dib, png })
    }
}

fn validate_dib(bytes: &[u8]) -> Result<(), ClipboardError> {
    const BITMAP_INFO_HEADER_SIZE: usize = 40;
    const BITMAP_V5_HEADER_SIZE: usize = 124;

    if bytes.len() < BITMAP_INFO_HEADER_SIZE {
        return Err(ClipboardError::InvalidImageData("DIB header is too short"));
    }

    let header_size = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if header_size < BITMAP_INFO_HEADER_SIZE || header_size > bytes.len() {
        return Err(ClipboardError::InvalidImageData(
            "DIB header size is invalid",
        ));
    }

    if header_size > BITMAP_V5_HEADER_SIZE {
        return Err(ClipboardError::InvalidImageData(
            "unsupported DIB header size",
        ));
    }

    Ok(())
}

fn validate_png(bytes: &[u8]) -> Result<(), ClipboardError> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

    if bytes.starts_with(PNG_SIGNATURE) {
        Ok(())
    } else {
        Err(ClipboardError::InvalidImageData("PNG signature is missing"))
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_dib, validate_png, ClipboardError};

    #[test]
    fn accepts_bitmap_info_header_dib() {
        let mut bytes = vec![0_u8; 40];
        bytes[0..4].copy_from_slice(&40_u32.to_le_bytes());

        assert!(validate_dib(&bytes).is_ok());
    }

    #[test]
    fn rejects_short_dib() {
        let error = validate_dib(&[0_u8; 4]).err();

        assert!(matches!(
            error,
            Some(ClipboardError::InvalidImageData("DIB header is too short"))
        ));
    }

    #[test]
    fn accepts_png_signature() {
        assert!(validate_png(b"\x89PNG\r\n\x1a\nextra").is_ok());
    }
}
