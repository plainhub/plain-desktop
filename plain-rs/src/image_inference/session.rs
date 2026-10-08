use std::{
    ffi::{CStr, CString, c_char, c_void},
    ptr::NonNull,
};
#[repr(C)]
pub struct Input {
    pub name: *const c_char,
    pub data: *mut c_void,
    pub bytes: usize,
    pub shape: *const i64,
    pub rank: usize,
    pub element_type: i32,
}
unsafe extern "C" {
    fn plain_onnx_context(
        library: *const c_char,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn plain_onnx_create(
        context: *mut c_void,
        model: *const c_char,
        threads: i32,
        provider: *const c_char,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn plain_onnx_close(session: *mut c_void);
    fn plain_onnx_run(
        session: *mut c_void,
        inputs: *mut Input,
        count: usize,
        output_name: *const c_char,
        output: *mut f32,
        dimensions: usize,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
}
pub struct Session(NonNull<c_void>);
unsafe impl Send for Session {}
impl Session {
    pub fn new(model: &std::path::Path) -> Result<Self, String> {
        #[cfg(target_os = "android")]
        let library = CString::new("libonnxruntime.so").unwrap();
        #[cfg(target_os = "ios")]
        let library = CString::new("linked ONNX Runtime").unwrap();
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let library = CString::new(library_path()).map_err(|e| e.to_string())?;
        let mut error = [0 as c_char; 2048];
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4)) as i32;
        #[cfg(unix)]
        let model = CString::new(std::os::unix::ffi::OsStrExt::as_bytes(model.as_os_str()))
            .map_err(|e| e.to_string())?;
        #[cfg(not(unix))]
        let model = CString::new(model.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        static CONTEXT: std::sync::OnceLock<Result<usize, String>> = std::sync::OnceLock::new();
        let context = CONTEXT
            .get_or_init(|| {
                let pointer = unsafe {
                    plain_onnx_context(library.as_ptr(), error.as_mut_ptr(), error.len())
                };
                if pointer.is_null() {
                    Err(unsafe { CStr::from_ptr(error.as_ptr()) }
                        .to_string_lossy()
                        .into_owned())
                } else {
                    Ok(pointer as usize)
                }
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let provider =
            CString::new(std::env::var("PLAIN_ONNX_PROVIDER").unwrap_or_else(|_| "CPU".into()))
                .map_err(|e| e.to_string())?;
        let mut pointer = unsafe {
            plain_onnx_create(
                *context as *mut c_void,
                model.as_ptr(),
                threads,
                provider.as_ptr(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if pointer.is_null() && provider.to_bytes() != b"CPU" {
            log::warn!(
                "ONNX provider initialization failed: {}",
                unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()
            );
            pointer = unsafe {
                plain_onnx_create(
                    *context as *mut c_void,
                    model.as_ptr(),
                    threads,
                    c"CPU".as_ptr(),
                    error.as_mut_ptr(),
                    error.len(),
                )
            };
        }
        NonNull::new(pointer).map(Self).ok_or_else(|| {
            unsafe { CStr::from_ptr(error.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        })
    }
    pub fn run(
        &mut self,
        inputs: &mut [Input],
        name: &str,
        dimensions: usize,
    ) -> Result<Vec<f32>, String> {
        let name = CString::new(name).map_err(|e| e.to_string())?;
        let mut values = vec![0.0; dimensions];
        let mut error = [0 as c_char; 2048];
        let code = unsafe {
            plain_onnx_run(
                self.0.as_ptr(),
                inputs.as_mut_ptr(),
                inputs.len(),
                name.as_ptr(),
                values.as_mut_ptr(),
                values.len(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if code != 0 {
            return Err(unsafe { CStr::from_ptr(error.as_ptr()) }
                .to_string_lossy()
                .into_owned());
        }
        super::normalize(&mut values)?;
        Ok(values)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            plain_onnx_close(self.0.as_ptr());
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn library_path() -> String {
    if let Ok(path) = std::env::var("PLAIN_ONNX_RUNTIME_LIBRARY") {
        return path;
    }
    let name = if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    };
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            for path in [
                directory.join(name),
                directory.join("../Resources").join(name),
            ] {
                if path.is_file() {
                    return path.to_string_lossy().into_owned();
                }
            }
        }
    }
    let development = std::path::Path::new(env!("PLAIN_ONNX_RUNTIME_LIBRARY"));
    if development.is_file() {
        development.to_string_lossy().into_owned()
    } else {
        name.into()
    }
}
