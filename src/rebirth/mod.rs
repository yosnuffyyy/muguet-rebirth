//! Project-specific adapters on the complete upstream Muguet runtime.
pub mod abi;
pub mod assets;
pub mod layout;
pub mod logs;
pub mod saves;
pub mod settings;
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
pub struct Launch {
    pub settings: settings::Settings,
    pub layout: layout::Layout,
    pub saves: PathBuf,
}
static LAUNCH: OnceLock<Launch> = OnceLock::new();
pub fn current() -> Option<&'static Launch> {
    LAUNCH.get()
}
pub fn prepare(root: &Path, json: &[u8], support: &Path) -> Result<()> {
    ensure!(
        LAUNCH.get().is_none(),
        "this process already owns a launch; stop and reopen before changing runtime settings"
    );
    let settings = settings::Settings::parse(json)?;
    let layout = layout::Layout::open(root)?;
    let saves = saves::prepare(support, settings.mode)?;
    LAUNCH
        .set(Launch {
            settings,
            layout,
            saves,
        })
        .map_err(|_| anyhow::anyhow!("launch already frozen"))?;
    Ok(())
}
pub fn apply() -> Result<Vec<String>> {
    let l = current().context("choose a game folder and validate settings before Play")?;
    let s = &l.settings;
    let (w, h) = s.resolution();
    std::env::set_var("MUGUET_DATA", &l.layout.data);
    std::env::set_var("MUGUET_SHADERS", &l.layout.shaders);
    std::env::set_var("MUGUET_FSROOT", &l.saves);
    std::env::set_var("MUGUET_RES", format!("{w}x{h}"));
    std::env::set_var("MUGUET_START_MODE", s.mode.to_string());
    std::env::set_var("MUGUET_TOUCH_SENS", s.touch_sensitivity.to_string());
    std::env::set_var("MUGUET_MOUSE_SENS", s.controller_sensitivity.to_string());
    if s.show_fps {
        std::env::set_var("MUGUET_SHOW_FPS", "1");
    } else {
        std::env::remove_var("MUGUET_SHOW_FPS");
    }
    // This fork's launch contract is offline. Existing network code stays
    // upstream-owned, but cannot be enabled by stale environment/config values.
    for name in ["MUGUET_HOST", "MUGUET_SERVER", "MUGUET_MUTE"] {
        std::env::remove_var(name);
    }
    let args = s.engine_args()?;
    eprintln!("[rebirth] frozen launch: {}", serde_json::to_string(s)?);
    eprintln!("[rebirth] runtime Muguet/Wasmtime49/wgpu30; original shader build {}; internal {w}x{h}; save namespace {}; offline",layout::ORIGINAL_BUILD,assets::save_namespace(s.mode)?);
    eprintln!("[rebirth] requested engine overrides: {} (runtime readback/gameplay still requires verification)",args.join(" "));
    Ok(args)
}
#[cfg(target_os = "ios")]
mod ffi {
    use super::*;
    use std::ffi::{c_char, CStr, CString};
    use std::sync::Mutex;
    static ERROR: Mutex<Option<CString>> = Mutex::new(None);
    #[no_mangle]
    pub unsafe extern "C" fn rebirth_prepare(
        root: *const c_char,
        json: *const c_char,
        support: *const c_char,
    ) -> bool {
        let r = (|| {
            ensure!(
                !root.is_null() && !json.is_null() && !support.is_null(),
                "null launcher argument"
            );
            prepare(
                Path::new(CStr::from_ptr(root).to_str()?),
                CStr::from_ptr(json).to_bytes(),
                Path::new(CStr::from_ptr(support).to_str()?),
            )
        })();
        match r {
            Ok(()) => {
                *ERROR.lock().unwrap() = None;
                true
            }
            Err(e) => {
                *ERROR.lock().unwrap() = CString::new(format!("{e:#}")).ok();
                false
            }
        }
    }
    #[no_mangle]
    pub unsafe extern "C" fn rebirth_logs_start(root: *const c_char) -> bool {
        let result = (|| {
            ensure!(!root.is_null(), "null log root");
            logs::start(Path::new(CStr::from_ptr(root).to_str()?))
        })();
        match result {
            Ok(()) => true,
            Err(e) => {
                *ERROR.lock().unwrap() = CString::new(format!("{e:#}")).ok();
                false
            }
        }
    }
    static EXPORTED: Mutex<Option<CString>> = Mutex::new(None);
    #[no_mangle]
    pub unsafe extern "C" fn rebirth_logs_export(root: *const c_char) -> *const c_char {
        let result = (|| {
            ensure!(!root.is_null(), "null export root");
            logs::export(Path::new(CStr::from_ptr(root).to_str()?))
        })();
        match result {
            Ok(path) => {
                let mut text = EXPORTED.lock().unwrap();
                *text = CString::new(path.to_string_lossy().as_bytes()).ok();
                text.as_ref().map_or(std::ptr::null(), |s| s.as_ptr())
            }
            Err(e) => {
                *ERROR.lock().unwrap() = CString::new(format!("{e:#}")).ok();
                std::ptr::null()
            }
        }
    }
    #[no_mangle]
    pub extern "C" fn rebirth_error() -> *const c_char {
        ERROR
            .lock()
            .unwrap()
            .as_ref()
            .map_or(std::ptr::null(), |e| e.as_ptr())
    }
}
