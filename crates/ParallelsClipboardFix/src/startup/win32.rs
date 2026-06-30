#[cfg(not(windows))]
use std::path::Path;

#[cfg(not(windows))]
use crate::startup::StartupError;

#[cfg(not(windows))]
pub fn create_shortcut(_executable: &Path, _shortcut: &Path) -> Result<(), StartupError> {
    Err(StartupError::UnsupportedPlatform)
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_void;
    use std::fs;
    use std::path::Path;
    use std::ptr::{self, NonNull};

    use crate::startup::StartupError;

    const CLSCTX_INPROC_SERVER: u32 = 0x1;
    const COINIT_APARTMENTTHREADED: u32 = 0x2;
    const S_OK: i32 = 0;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    const CLSID_SHELL_LINK: Guid = Guid {
        data1: 0x00021401,
        data2: 0x0000,
        data3: 0x0000,
        data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };

    const IID_ISHELL_LINK_W: Guid = Guid {
        data1: 0x000214F9,
        data2: 0x0000,
        data3: 0x0000,
        data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };

    const IID_IPERSIST_FILE: Guid = Guid {
        data1: 0x0000010B,
        data2: 0x0000,
        data3: 0x0000,
        data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };

    #[repr(C)]
    struct IShellLinkW {
        vtable: *const IShellLinkWVtbl,
    }

    #[repr(C)]
    struct IShellLinkWVtbl {
        query_interface:
            unsafe extern "system" fn(*mut IShellLinkW, *const Guid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut IShellLinkW) -> u32,
        release: unsafe extern "system" fn(*mut IShellLinkW) -> u32,
        get_path: usize,
        get_id_list: usize,
        set_id_list: usize,
        get_description: usize,
        set_description: unsafe extern "system" fn(*mut IShellLinkW, *const u16) -> i32,
        get_working_directory: usize,
        set_working_directory: unsafe extern "system" fn(*mut IShellLinkW, *const u16) -> i32,
        get_arguments: usize,
        set_arguments: usize,
        get_hotkey: usize,
        set_hotkey: usize,
        get_show_cmd: usize,
        set_show_cmd: usize,
        get_icon_location: usize,
        set_icon_location: usize,
        set_relative_path: usize,
        resolve: usize,
        set_path: unsafe extern "system" fn(*mut IShellLinkW, *const u16) -> i32,
    }

    #[repr(C)]
    struct IPersistFile {
        vtable: *const IPersistFileVtbl,
    }

    #[repr(C)]
    struct IPersistFileVtbl {
        query_interface:
            unsafe extern "system" fn(*mut IPersistFile, *const Guid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut IPersistFile) -> u32,
        release: unsafe extern "system" fn(*mut IPersistFile) -> u32,
        get_class_id: usize,
        is_dirty: usize,
        load: usize,
        save: unsafe extern "system" fn(*mut IPersistFile, *const u16, i32) -> i32,
        save_completed: usize,
        get_cur_file: usize,
    }

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, init: u32) -> i32;
        fn CoUninitialize();
        fn CoCreateInstance(
            class_id: *const Guid,
            outer: *mut c_void,
            context: u32,
            interface_id: *const Guid,
            object: *mut *mut c_void,
        ) -> i32;
    }

    pub fn create_shortcut(executable: &Path, shortcut: &Path) -> Result<(), StartupError> {
        if let Some(parent) = shortcut.parent() {
            fs::create_dir_all(parent)?;
        }

        unsafe {
            let _com = ComApartment::initialize()?;
            let shell_link = ShellLink::create()?;

            shell_link.set_path(executable)?;
            shell_link.set_description("ParallelsClipboardFix")?;

            if let Some(working_directory) = executable.parent() {
                shell_link.set_working_directory(working_directory)?;
            }

            shell_link.save(shortcut)
        }
    }

    struct ComApartment;

    impl ComApartment {
        unsafe fn initialize() -> Result<Self, StartupError> {
            let result = unsafe { CoInitializeEx(ptr::null_mut(), COINIT_APARTMENTTHREADED) };
            if result < S_OK {
                return Err(StartupError::ComFailed("CoInitializeEx"));
            }
            Ok(Self)
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    struct ShellLink {
        pointer: NonNull<IShellLinkW>,
    }

    impl ShellLink {
        unsafe fn create() -> Result<Self, StartupError> {
            let mut object = ptr::null_mut();
            let result = unsafe {
                CoCreateInstance(
                    &CLSID_SHELL_LINK,
                    ptr::null_mut(),
                    CLSCTX_INPROC_SERVER,
                    &IID_ISHELL_LINK_W,
                    &mut object,
                )
            };
            if result < S_OK {
                return Err(StartupError::ComFailed("CoCreateInstance"));
            }

            let pointer =
                NonNull::new(object.cast()).ok_or(StartupError::ComFailed("IShellLinkW"))?;
            Ok(Self { pointer })
        }

        unsafe fn set_path(&self, path: &Path) -> Result<(), StartupError> {
            let path = wide_path(path)?;
            let result = unsafe {
                ((*(*self.pointer.as_ptr()).vtable).set_path)(self.pointer.as_ptr(), path.as_ptr())
            };
            hresult(result, "IShellLinkW::SetPath")
        }

        unsafe fn set_working_directory(&self, path: &Path) -> Result<(), StartupError> {
            let path = wide_path(path)?;
            let result = unsafe {
                ((*(*self.pointer.as_ptr()).vtable).set_working_directory)(
                    self.pointer.as_ptr(),
                    path.as_ptr(),
                )
            };
            hresult(result, "IShellLinkW::SetWorkingDirectory")
        }

        unsafe fn set_description(&self, description: &str) -> Result<(), StartupError> {
            let description = wide_null(description);
            let result = unsafe {
                ((*(*self.pointer.as_ptr()).vtable).set_description)(
                    self.pointer.as_ptr(),
                    description.as_ptr(),
                )
            };
            hresult(result, "IShellLinkW::SetDescription")
        }

        unsafe fn save(&self, shortcut: &Path) -> Result<(), StartupError> {
            let mut object = ptr::null_mut();
            let query_result = unsafe {
                ((*(*self.pointer.as_ptr()).vtable).query_interface)(
                    self.pointer.as_ptr(),
                    &IID_IPERSIST_FILE,
                    &mut object,
                )
            };
            hresult(query_result, "IShellLinkW::QueryInterface(IPersistFile)")?;

            let persist = NonNull::new(object.cast::<IPersistFile>())
                .ok_or(StartupError::ComFailed("IPersistFile"))?;
            let path = wide_path(shortcut)?;
            let save_result =
                unsafe { ((*(*persist.as_ptr()).vtable).save)(persist.as_ptr(), path.as_ptr(), 1) };
            unsafe {
                ((*(*persist.as_ptr()).vtable).release)(persist.as_ptr());
            }

            hresult(save_result, "IPersistFile::Save")
        }
    }

    impl Drop for ShellLink {
        fn drop(&mut self) {
            unsafe {
                ((*(*self.pointer.as_ptr()).vtable).release)(self.pointer.as_ptr());
            }
        }
    }

    fn hresult(result: i32, operation: &'static str) -> Result<(), StartupError> {
        if result < S_OK {
            Err(StartupError::ComFailed(operation))
        } else {
            Ok(())
        }
    }

    fn wide_path(path: &Path) -> Result<Vec<u16>, StartupError> {
        let Some(path) = path.to_str() else {
            return Err(StartupError::InvalidPath);
        };
        Ok(wide_null(path))
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(windows)]
pub use windows_impl::create_shortcut;
