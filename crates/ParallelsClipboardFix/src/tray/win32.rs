#[cfg(not(windows))]
use std::sync::mpsc::Sender;

#[cfg(not(windows))]
use crate::tray::{TrayCommand, TrayError, TrayState};

#[cfg(not(windows))]
#[derive(Debug)]
pub struct TrayHandle;

#[cfg(not(windows))]
impl TrayHandle {
    pub fn is_running(&self) -> bool {
        false
    }
}

#[cfg(not(windows))]
pub fn start_tray(
    _sender: Sender<TrayCommand>,
    _state: TrayState,
) -> Result<TrayHandle, TrayError> {
    Err(TrayError::UnsupportedPlatform)
}

#[cfg(windows)]
mod windows_impl {
    #![allow(unsafe_op_in_unsafe_fn)]

    use std::ffi::c_void;
    use std::mem;
    use std::ptr;
    use std::sync::mpsc::{self, Sender};
    use std::thread::{self, JoinHandle};

    use crate::tray::{MenuItemId, TrayCommand, TrayError, TrayState};

    const HWND_MESSAGE_VALUE: isize = -3;
    const GWLP_USERDATA: i32 = -21;
    const ID_TRAY_ICON: u32 = 1;
    const NIF_MESSAGE: u32 = 0x00000001;
    const NIF_ICON: u32 = 0x00000002;
    const NIF_TIP: u32 = 0x00000004;
    const NIM_ADD: u32 = 0x00000000;
    const NIM_DELETE: u32 = 0x00000002;
    const TPM_RIGHTBUTTON: u32 = 0x0002;
    const WM_APP_TRAY: u32 = 0x8000 + 1;
    const WM_CLOSE: u32 = 0x0010;
    const WM_COMMAND: u32 = 0x0111;
    const WM_NCCREATE: u32 = 0x0081;
    const WM_DESTROY: u32 = 0x0002;
    const WM_RBUTTONUP: usize = 0x0205;
    const MF_STRING: u32 = 0x0000;
    const MF_SEPARATOR: u32 = 0x0800;

    type Bool = i32;
    type Hicon = *mut c_void;
    type Hinstance = *mut c_void;
    type Hmenu = *mut c_void;
    type Hwnd = *mut c_void;
    type Lparam = isize;
    type Lresult = isize;
    type Wparam = usize;

    #[repr(C)]
    struct WndClassW {
        style: u32,
        lpfn_wnd_proc: Option<unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam) -> Lresult>,
        cb_cls_extra: i32,
        cb_wnd_extra: i32,
        h_instance: Hinstance,
        h_icon: Hicon,
        h_cursor: *mut c_void,
        hbr_background: *mut c_void,
        lpsz_menu_name: *const u16,
        lpsz_class_name: *const u16,
    }

    #[repr(C)]
    struct NotifyIconDataW {
        cb_size: u32,
        hwnd: Hwnd,
        uid: u32,
        flags: u32,
        callback_message: u32,
        icon: Hicon,
        tip: [u16; 128],
        state: u32,
        state_mask: u32,
        info: [u16; 256],
        timeout_or_version: u32,
        info_title: [u16; 64],
        info_flags: u32,
        guid_item: [u8; 16],
        balloon_icon: Hicon,
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
        fn CreatePopupMenu() -> Hmenu;
        fn AppendMenuW(menu: Hmenu, flags: u32, item_id: usize, text: *const u16) -> Bool;
        fn TrackPopupMenu(
            menu: Hmenu,
            flags: u32,
            x: i32,
            y: i32,
            reserved: i32,
            hwnd: Hwnd,
            rect: *const c_void,
        ) -> Bool;
        fn DestroyMenu(menu: Hmenu) -> Bool;
        fn GetCursorPos(point: *mut Point) -> Bool;
        fn LoadIconW(instance: Hinstance, icon_name: *const u16) -> Hicon;
    }

    #[link(name = "shell32")]
    extern "system" {
        fn Shell_NotifyIconW(message: u32, data: *mut NotifyIconDataW) -> Bool;
    }

    #[derive(Debug)]
    pub struct TrayHandle {
        hwnd: Hwnd,
        thread: Option<JoinHandle<()>>,
    }

    impl TrayHandle {
        pub fn is_running(&self) -> bool {
            !self.hwnd.is_null()
        }
    }

    impl Drop for TrayHandle {
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

    pub fn start_tray(
        sender: Sender<TrayCommand>,
        state: TrayState,
    ) -> Result<TrayHandle, TrayError> {
        let (hwnd_sender, hwnd_receiver) = mpsc::sync_channel(1);

        let thread = thread::Builder::new()
            .name("ParallelsClipboardFix-tray".to_owned())
            .spawn(move || {
                let hwnd = unsafe { create_tray_window(sender, state) };

                if let Some(hwnd) = hwnd {
                    let _ = hwnd_sender.send(Some(hwnd as usize));
                    unsafe {
                        message_loop();
                    }
                } else {
                    let _ = hwnd_sender.send(None);
                }
            })
            .map_err(|_| TrayError::WindowThreadFailed)?;

        let hwnd = hwnd_receiver
            .recv()
            .map_err(|_| TrayError::WindowThreadFailed)?
            .map(|hwnd| hwnd as Hwnd)
            .ok_or(TrayError::WindowCreationFailed)?;

        Ok(TrayHandle {
            hwnd,
            thread: Some(thread),
        })
    }

    unsafe fn create_tray_window(sender: Sender<TrayCommand>, state: TrayState) -> Option<Hwnd> {
        let instance = GetModuleHandleW(ptr::null());
        let class_name = wide_null("ClipboardBridgeTray");
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

        let tray_state = Box::new(WindowState { sender, state });
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
            Box::into_raw(tray_state).cast(),
        );

        if hwnd.is_null() {
            return None;
        }

        if add_icon(hwnd) {
            Some(hwnd)
        } else {
            DestroyWindow(hwnd);
            None
        }
    }

    unsafe fn add_icon(hwnd: Hwnd) -> bool {
        let mut data = mem::zeroed::<NotifyIconDataW>();
        data.cb_size = mem::size_of::<NotifyIconDataW>() as u32;
        data.hwnd = hwnd;
        data.uid = ID_TRAY_ICON;
        data.flags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.callback_message = WM_APP_TRAY;
        data.icon = LoadIconW(ptr::null_mut(), 32512_usize as *const u16);
        write_tip(&mut data.tip, "ParallelsClipboardFix");

        Shell_NotifyIconW(NIM_ADD, &mut data) != 0
    }

    unsafe fn remove_icon(hwnd: Hwnd) {
        let mut data = mem::zeroed::<NotifyIconDataW>();
        data.cb_size = mem::size_of::<NotifyIconDataW>() as u32;
        data.hwnd = hwnd;
        data.uid = ID_TRAY_ICON;
        Shell_NotifyIconW(NIM_DELETE, &mut data);
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
            remove_icon(hwnd);
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !state_ptr.is_null() {
                drop(Box::from_raw(state_ptr));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            PostQuitMessage(0);
            return 0;
        }

        if message == WM_NCCREATE {
            let create_struct = lparam as *const CreateStructW;
            if !create_struct.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create_struct).create_params as isize);
            }
            return 1;
        }

        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        if state_ptr.is_null() {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }

        let state = &mut *state_ptr;
        match message {
            WM_APP_TRAY if lparam as usize == WM_RBUTTONUP => {
                show_menu(hwnd, state.state);
                0
            }
            WM_COMMAND => {
                let item_id = (wparam & 0xffff) as u16;
                if let Some(menu_item) = MenuItemId::from_u16(item_id) {
                    let _ = state.sender.send(menu_item.command());
                }
                0
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    unsafe fn show_menu(hwnd: Hwnd, state: TrayState) {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }

        append_menu(menu, MenuItemId::FixNow, "Fix clipboard now");
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        append_menu(
            menu,
            MenuItemId::ToggleEnabled,
            if state.enabled {
                "Disable fixer"
            } else {
                "Enable fixer"
            },
        );
        append_menu(
            menu,
            MenuItemId::ToggleStartup,
            if state.start_with_windows {
                "Disable start with Windows"
            } else {
                "Start with Windows"
            },
        );
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        append_menu(menu, MenuItemId::Quit, "Quit");

        let mut point = mem::zeroed::<Point>();
        if GetCursorPos(&mut point) != 0 {
            TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                hwnd,
                ptr::null(),
            );
        }
        DestroyMenu(menu);
    }

    unsafe fn append_menu(menu: Hmenu, id: MenuItemId, label: &str) {
        let text = wide_null(label);
        AppendMenuW(menu, MF_STRING, id as usize, text.as_ptr());
    }

    struct WindowState {
        sender: Sender<TrayCommand>,
        state: TrayState,
    }

    fn write_tip(buffer: &mut [u16; 128], value: &str) {
        for (index, code_unit) in value.encode_utf16().take(buffer.len() - 1).enumerate() {
            buffer[index] = code_unit;
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(windows)]
pub use windows_impl::{start_tray, TrayHandle};
