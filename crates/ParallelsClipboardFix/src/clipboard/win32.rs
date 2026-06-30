#[cfg(not(windows))]
use crate::clipboard::{ClipboardError, FixOutcome};

#[cfg(not(windows))]
use crate::config::AppConfig;

#[cfg(not(windows))]
pub fn normalize_clipboard_image(_config: &AppConfig) -> Result<FixOutcome, ClipboardError> {
    Err(ClipboardError::UnsupportedPlatform)
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_void;
    use std::ptr::{self, NonNull};
    use std::slice;

    use crate::clipboard::{ClipboardError, ClipboardImage, FixOutcome};
    use crate::config::AppConfig;

    const CF_DIB: u32 = 8;
    const CF_DIBV5: u32 = 17;
    const GMEM_MOVEABLE: u32 = 0x0002;

    const PNG_FORMAT_NAME: &[u16] = &['P' as u16, 'N' as u16, 'G' as u16, 0];

    type Hwnd = *mut c_void;
    type Handle = *mut c_void;

    #[link(name = "user32")]
    extern "system" {
        fn OpenClipboard(hwnd_new_owner: Hwnd) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn IsClipboardFormatAvailable(format: u32) -> i32;
        fn GetClipboardData(format: u32) -> Handle;
        fn SetClipboardData(format: u32, handle: Handle) -> Handle;
        fn RegisterClipboardFormatW(format: *const u16) -> u32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> Handle;
        fn GlobalLock(handle: Handle) -> *mut c_void;
        fn GlobalUnlock(handle: Handle) -> i32;
        fn GlobalSize(handle: Handle) -> usize;
        fn GlobalFree(handle: Handle) -> Handle;
    }

    pub fn normalize_clipboard_image(config: &AppConfig) -> Result<FixOutcome, ClipboardError> {
        let png_format = registered_png_format();
        let _clipboard = ClipboardGuard::open()?;

        let image = read_image(png_format)?;
        let Some(image) = image else {
            return Ok(FixOutcome::NoReadableImage);
        };

        unsafe {
            if EmptyClipboard() == 0 {
                return Err(ClipboardError::EmptyClipboardFailed);
            }
        }

        let mut wrote_dib = false;
        let mut wrote_png = false;

        if config.write_dib {
            if let Some(dib) = image.dib.as_deref() {
                write_bytes(CF_DIB, dib, "CF_DIB")?;
                wrote_dib = true;
            }
        }

        if config.write_png {
            if let Some(png) = image.png.as_deref() {
                write_bytes(png_format, png, "PNG")?;
                wrote_png = true;
            }
        }

        if wrote_dib || wrote_png {
            Ok(FixOutcome::Fixed {
                wrote_dib,
                wrote_png,
            })
        } else {
            Ok(FixOutcome::NoReadableImage)
        }
    }

    fn read_image(png_format: u32) -> Result<Option<ClipboardImage>, ClipboardError> {
        let dib = if format_available(CF_DIBV5) {
            Some(read_bytes(CF_DIBV5, "CF_DIBV5")?)
        } else if format_available(CF_DIB) {
            Some(read_bytes(CF_DIB, "CF_DIB")?)
        } else {
            None
        };

        let png = if png_format != 0 && format_available(png_format) {
            Some(read_bytes(png_format, "PNG")?)
        } else {
            None
        };

        if dib.is_none() && png.is_none() {
            return Ok(None);
        }

        ClipboardImage::new(dib, png).map(Some)
    }

    fn format_available(format: u32) -> bool {
        unsafe { IsClipboardFormatAvailable(format) != 0 }
    }

    fn read_bytes(format: u32, format_name: &'static str) -> Result<Vec<u8>, ClipboardError> {
        let handle = unsafe { GetClipboardData(format) };
        let handle = NonNull::new(handle).ok_or(ClipboardError::ReadFormatFailed(format_name))?;

        let size = unsafe { GlobalSize(handle.as_ptr()) };
        if size == 0 {
            return Err(ClipboardError::ReadFormatFailed(format_name));
        }

        let locked = unsafe { GlobalLock(handle.as_ptr()) };
        let locked = NonNull::new(locked).ok_or(ClipboardError::ReadFormatFailed(format_name))?;

        let bytes = unsafe { slice::from_raw_parts(locked.as_ptr().cast::<u8>(), size) }.to_vec();
        unsafe {
            GlobalUnlock(handle.as_ptr());
        }

        Ok(bytes)
    }

    fn write_bytes(
        format: u32,
        bytes: &[u8],
        format_name: &'static str,
    ) -> Result<(), ClipboardError> {
        let allocation = ClipboardAllocation::new(bytes)?;
        let handle = allocation.into_raw();

        let result = unsafe { SetClipboardData(format, handle) };
        if result.is_null() {
            unsafe {
                GlobalFree(handle);
            }
            return Err(ClipboardError::WriteFormatFailed(format_name));
        }

        Ok(())
    }

    fn registered_png_format() -> u32 {
        unsafe { RegisterClipboardFormatW(PNG_FORMAT_NAME.as_ptr()) }
    }

    struct ClipboardGuard;

    impl ClipboardGuard {
        fn open() -> Result<Self, ClipboardError> {
            let opened = unsafe { OpenClipboard(ptr::null_mut()) };
            if opened == 0 {
                return Err(ClipboardError::OpenClipboardFailed);
            }
            Ok(Self)
        }
    }

    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }

    struct ClipboardAllocation {
        handle: NonNull<c_void>,
    }

    impl ClipboardAllocation {
        fn new(bytes: &[u8]) -> Result<Self, ClipboardError> {
            let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) };
            let handle = NonNull::new(handle).ok_or(ClipboardError::AllocationFailed)?;

            let locked = unsafe { GlobalLock(handle.as_ptr()) };
            let Some(locked) = NonNull::new(locked) else {
                unsafe {
                    GlobalFree(handle.as_ptr());
                }
                return Err(ClipboardError::AllocationFailed);
            };

            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), locked.as_ptr().cast::<u8>(), bytes.len());
                GlobalUnlock(handle.as_ptr());
            }

            Ok(Self { handle })
        }

        fn into_raw(self) -> Handle {
            let handle = self.handle.as_ptr();
            std::mem::forget(self);
            handle
        }
    }

    impl Drop for ClipboardAllocation {
        fn drop(&mut self) {
            unsafe {
                GlobalFree(self.handle.as_ptr());
            }
        }
    }
}

#[cfg(windows)]
pub use windows_impl::normalize_clipboard_image;
