use std::collections::HashSet;
use std::sync::OnceLock;

/// Cached capabilities of the underlying GLES implementation.
#[derive(Debug, Clone)]
pub struct GlesCaps {
    pub version_major: u32,
    pub version_minor: u32,
    pub extensions: HashSet<String>,
}

impl GlesCaps {
    pub fn has_extension(&self, ext: &str) -> bool {
        self.extensions.contains(ext)
    }

    pub fn is_es32(&self) -> bool {
        self.version_major == 3 && self.version_minor >= 2
    }

    pub fn is_es31_plus(&self) -> bool {
        self.version_major == 3 && self.version_minor >= 1
    }
}

static GLES_CAPS: OnceLock<GlesCaps> = OnceLock::new();

/// Probe GLES version and extension list. Must be called after GLES is loaded
/// and a context is current. Safe to call multiple times: only the first
/// successful probe is retained.
pub fn probe_and_set(dispatch: &crate::backend::dispatch::GlesDispatch) {
    let _ = GLES_CAPS.get_or_init(|| {
        let caps = unsafe { probe(dispatch) };
        log::info!(
            "[FluorateGL] GLES caps: {}.{} with {} extensions",
            caps.version_major,
            caps.version_minor,
            caps.extensions.len()
        );
        caps
    });
}

unsafe fn probe(dispatch: &crate::backend::dispatch::GlesDispatch) -> GlesCaps {
    let version_ptr = unsafe { (dispatch.get_string)(0x1F02) }; // GL_VERSION
    let (major, minor) = parse_version(c_str_to_string(version_ptr));

    let mut extensions = HashSet::new();

    // Try the modern GL_NUM_EXTENSIONS / glGetStringi path first.
    let mut num_exts = 0i32;
    unsafe { (dispatch.get_integerv)(0x821D, &mut num_exts) }; // GL_NUM_EXTENSIONS
    for i in 0..num_exts as u32 {
        let ext_ptr = unsafe { (dispatch.get_string_i)(0x1F03, i) }; // GL_EXTENSIONS
        if !ext_ptr.is_null() {
            extensions.insert(c_str_to_string(ext_ptr));
        }
    }

    // Fallback to the space-separated GL_EXTENSIONS string.
    if extensions.is_empty() {
        let ext_ptr = unsafe { (dispatch.get_string)(0x1F03) }; // GL_EXTENSIONS
        let ext_string = c_str_to_string(ext_ptr);
        for ext in ext_string.split_whitespace() {
            extensions.insert(ext.to_string());
        }
    }

    GlesCaps {
        version_major: major,
        version_minor: minor,
        extensions,
    }
}

fn parse_version(s: String) -> (u32, u32) {
    // GLES version strings look like "OpenGL ES 3.2 ...".
    let mut major = 3u32;
    let mut minor = 0u32;
    for token in s.split_whitespace() {
        if let Some(dot) = token.find('.') {
            if let (Ok(ma), Ok(mi)) = (
                token[..dot].parse::<u32>(),
                token[dot + 1..].parse::<u32>(),
            ) {
                major = ma;
                minor = mi;
                break;
            }
        }
    }
    (major, minor)
}

fn c_str_to_string(ptr: *const libc::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe {
        std::ffi::CStr::from_ptr(ptr)
            .to_string_lossy()
            .into_owned()
    }
}

/// Return cached GLES caps. If not probed yet, attempt a lazy probe using the
/// current GLES dispatch table. This only works when a GLES context is current.
pub fn get() -> Option<&'static GlesCaps> {
    GLES_CAPS.get().or_else(|| {
        crate::backend::with_gles_dispatch(|dispatch| {
            probe_and_set(dispatch);
        });
        GLES_CAPS.get()
    })
}
