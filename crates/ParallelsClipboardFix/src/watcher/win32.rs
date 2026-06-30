#[cfg(not(windows))]
use std::sync::mpsc::Sender;

#[cfg(not(windows))]
use crate::watcher::{RetrySchedule, WatcherError, WatcherEvent};

#[cfg(not(windows))]
#[derive(Debug)]
pub struct WatcherHandle;

#[cfg(not(windows))]
impl WatcherHandle {
    pub fn is_running(&self) -> bool {
        false
    }
}

#[cfg(not(windows))]
pub fn start_watcher(
    _sender: Sender<WatcherEvent>,
    _retry_schedule: RetrySchedule,
) -> Result<WatcherHandle, WatcherError> {
    Err(WatcherError::UnsupportedPlatform)
}

#[cfg(windows)]
mod windows_impl {
    #![allow(unsafe_op_in_unsafe_fn)]

    use std::ffi::c_void;
    use std::mem;
    use std::ptr;
    use std::sync::mpsc::{self, Sender};
    use std::thread::{self, JoinHandle};

    use crate::watcher::{RetrySchedule, WatcherError, WatcherEvent};

    const HWND_MESSAGE_VALUE: isize = -3;
    const GWLP_USERDATA: i32 = -21;
    const WM_CLIPBOARDUPDATE: u32 = 0x031D;
    const WM_CLOSE: u32 = 0x0010;
    const WM_NCCREATE: u32 = 0x0081;
    const WM_DESTROY: u32 = 0x0002;
    const WM_TIMER: u32 = 0x0113;

    type Bool = i32;
    type Hinstance = *mut c_void;
    type Hwnd = *mut c_void;
    type Lparam = isize;
    type Lresult = isize;
    type UintPtr = usize;
    type Wparam = usize;

    #[repr(C)]
    struct WndClassW {
        style: u32,
        lpfn_wnd_proc: Option<unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam) -> Lresult>,
        cb_cls_extra: i32,
        cb_wnd_extra: i32,
        h_instance: Hinstance,
        h_icon: *mut c_void,
        h_cursor: *mut c_void,
        hbr_background: *mut c_void,
        lpsz_menu_name: *const u16,
        lpsz_class_name: *const u16,
    }

    #[repr(C)]
    struct Msg {
        hwnd: Hwnd,
        message: u32,
        w_param: Wparam,
        l_param: Lparam,
        time: u32,
        point: Point,
    }

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct CreateStructW {
        create_params: *mut c_void,
        instance: Hinstance,
        menu: *mut c_void,
        parent: Hwnd,
        cy: i32,
        cx: i32,
        y: i32,
        x: i32,
        style: i32,
        name: *const u16,
        class: *const u16,
        extended_style: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(module_name: *const u16) -> Hinstance;
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterClassW(window_class: *const WndClassW) -> u16;
        fn CreateWindowExW(
            extended_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: Hwnd,
            menu: *mut c_void,
            instance: Hinstance,
            parameter: *mut c_void,
        ) -> Hwnd;
        fn DestroyWindow(hwnd: Hwnd) -> Bool;
        fn DefWindowProcW(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
        fn GetMessageW(message: *mut Msg, hwnd: Hwnd, min_filter: u32, max_filter: u32) -> Bool;
        fn TranslateMessage(message: *const Msg) -> Bool;
        fn DispatchMessageW(message: *const Msg) -> Lresult;
        fn PostMessageW(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Bool;
        fn PostQuitMessage(exit_code: i32);
        fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, value: isize) -> isize;
        fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
        fn AddClipboardFormatListener(hwnd: Hwnd) -> Bool;
        fn RemoveClipboardFormatListener(hwnd: Hwnd) -> Bool;
        fn SetTimer(
            hwnd: Hwnd,
            event_id: UintPtr,
            milliseconds: u32,
            timer_proc: *mut c_void,
        ) -> UintPtr;
        fn KillTimer(hwnd: Hwnd, event_id: UintPtr) -> Bool;
    }

    #[derive(Debug)]
    pub struct WatcherHandle {
        hwnd: Hwnd,
        thread: Option<JoinHandle<()>>,
    }

    impl WatcherHandle {
        pub fn is_running(&self) -> bool {
            !self.hwnd.is_null()
        }
    }

    impl Drop for WatcherHandle {
        fn drop(&mut self) {
            if !self.hwnd.is_null() {
                unsafe {
                    PostMessageW(self.hwnd, WM_CLOSE, 0, 0);
                }
            }

            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    pub fn start_watcher(
        sender: Sender<WatcherEvent>,
        retry_schedule: RetrySchedule,
    ) -> Result<WatcherHandle, WatcherError> {
        let (hwnd_sender, hwnd_receiver) = mpsc::sync_channel(1);

        let thread = thread::Builder::new()
            .name("clipboard-watcher".to_owned())
            .spawn(move || {
                let hwnd = unsafe { create_message_window(sender, retry_schedule) };

                if let Some(hwnd) = hwnd {
                    let _ = hwnd_sender.send(Some(hwnd as usize));
                    unsafe {
                        message_loop();
                    }
                } else {
                    let _ = hwnd_sender.send(None);
                }
            })
            .map_err(|_| WatcherError::WindowThreadFailed)?;

        let hwnd = hwnd_receiver
            .recv()
            .map_err(|_| WatcherError::WindowThreadFailed)?
            .map(|hwnd| hwnd as Hwnd)
            .ok_or(WatcherError::WindowCreationFailed)?;

        Ok(WatcherHandle {
            hwnd,
            thread: Some(thread),
        })
    }

    unsafe fn create_message_window(
        sender: Sender<WatcherEvent>,
        retry_schedule: RetrySchedule,
    ) -> Option<Hwnd> {
        let instance = GetModuleHandleW(ptr::null());
        let class_name = wide_null("ClipboardBridgeWatcher");
        let window_class = WndClassW {
            style: 0,
            lpfn_wnd_proc: Some(window_proc),
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: instance,
            h_icon: ptr::null_mut(),
            h_cursor: ptr::null_mut(),
            hbr_background: ptr::null_mut(),
            lpsz_menu_name: ptr::null(),
            lpsz_class_name: class_name.as_ptr(),
        };

        RegisterClassW(&window_class);

        let state = Box::new(WindowState {
            sender,
            retry_schedule,
        });

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE_VALUE as Hwnd,
            ptr::null_mut(),
            instance,
            Box::into_raw(state).cast(),
        );

        if hwnd.is_null() {
            return None;
        }

        if AddClipboardFormatListener(hwnd) == 0 {
            DestroyWindow(hwnd);
            return None;
        }

        Some(hwnd)
    }

    unsafe fn message_loop() {
        let mut message = mem::zeroed::<Msg>();
        while GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: Hwnd,
        message: u32,
        wparam: Wparam,
        lparam: Lparam,
    ) -> Lresult {
        if message == WM_CLOSE {
            DestroyWindow(hwnd);
            return 0;
        }

        if message == WM_DESTROY {
            RemoveClipboardFormatListener(hwnd);
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !state_ptr.is_null() {
                drop(Box::from_raw(state_ptr));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            PostQuitMessage(0);
            return 0;
        }

        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;

        if message == WM_NCCREATE {
            let create_struct = lparam as *const CreateStructW;
            if !create_struct.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create_struct).create_params as isize);
            }
            return 1;
        }

        if state_ptr.is_null() {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }

        let state = &mut *state_ptr;
        match message {
            WM_CLIPBOARDUPDATE => {
                let _ = state.sender.send(WatcherEvent::ClipboardChanged);
                state.schedule_retries(hwnd);
                0
            }
            WM_TIMER => {
                let attempt = wparam as u32;
                let _ = state.sender.send(WatcherEvent::RetryRequested { attempt });
                KillTimer(hwnd, wparam);
                0
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    struct WindowState {
        sender: Sender<WatcherEvent>,
        retry_schedule: RetrySchedule,
    }

    impl WindowState {
        unsafe fn schedule_retries(&self, hwnd: Hwnd) {
            let delay_ms = self.retry_schedule.delay().as_millis();
            let milliseconds = if delay_ms > u128::from(u32::MAX) {
                u32::MAX
            } else {
                delay_ms as u32
            };

            for attempt in 1..=self.retry_schedule.attempts() {
                let due = milliseconds.saturating_mul(attempt);
                SetTimer(hwnd, attempt as usize, due, ptr::null_mut());
            }
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(windows)]
pub use windows_impl::{start_watcher, WatcherHandle};
