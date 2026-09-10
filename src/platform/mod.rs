#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(not(debug_assertions))]
use crate::{config::Config, log};
#[cfg(not(debug_assertions))]
use std::process::exit;

#[cfg(not(debug_assertions))]
static mut GLOBAL_CALLBACK: Option<Box<dyn Fn()>> = None;

#[cfg(not(debug_assertions))]
extern "C" fn breakdown_signal_handler(sig: i32) {
    let mut stack = vec![];
    backtrace::trace(|frame| {
        backtrace::resolve_frame(frame, |symbol| {
            if let Some(name) = symbol.name() {
                stack.push(name.to_string());
            }
        });
        true // keep going to the next frame
    });
    let mut info = String::default();
    if stack.iter().any(|s| {
        s.contains(&"nouveau_pushbuf_kick")
            || s.to_lowercase().contains("nvidia")
            || s.contains("gdk_window_end_draw_frame")
            || s.contains("gdk_cairo_draw_from_gl")
            || s.contains("glGetString")
    }) {
        // Software rendering only works around the crash when the machine has a
        // software GL driver at all. Without one `LIBGL_ALWAYS_SOFTWARE` makes GL
        // initialization fail instead, and the window never comes up, which is
        // worse than the crash it was meant to fix.
        if Config::get_option("allow-always-software-render") == "Y" {
            // Already on and crashing anyway, so it is not the way out here.
            Config::set_option("allow-always-software-render".to_string(), "".to_string());
            info = "Software rendering did not help, turned it back off. Update the graphics \
                    driver, or point GL at another vendor, e.g. `__GLX_VENDOR_LIBRARY_NAME=mesa`."
                .to_string();
        } else if software_gl_available() {
            Config::set_option("allow-always-software-render".to_string(), "Y".to_string());
            info = "Always use software rendering will be set.".to_string();
        } else {
            info = "No software GL driver is installed, so software rendering cannot help here. \
                    Update the graphics driver instead."
                .to_string();
        }
        log::info!("{}", info);
    }
    if stack.iter().any(|s| {
        s.to_lowercase().contains("nvidia")
            || s.to_lowercase().contains("amf")
            || s.to_lowercase().contains("mfx")
            || s.contains("cuProfilerStop")
    }) {
        Config::set_option("enable-hwcodec".to_string(), "N".to_string());
        info = "Perhaps hwcodec causing the crash, disable it first".to_string();
        log::info!("{}", info);
    }
    log::error!(
        "Got signal {} and exit. stack:\n{}",
        sig,
        stack.join("\n").to_string()
    );
    if !info.is_empty() {
        #[cfg(target_os = "linux")]
        linux::system_message(
            "RustDesk",
            &format!("Got signal {} and exit.{}", sig, info),
            true,
        )
        .ok();
    }
    unsafe {
        #[allow(static_mut_refs)]
        if let Some(callback) = &GLOBAL_CALLBACK {
            callback()
        }
    }
    exit(0);
}

#[cfg(not(debug_assertions))]
pub fn register_breakdown_handler<T>(callback: T)
where
    T: Fn() + 'static,
{
    unsafe {
        GLOBAL_CALLBACK = Some(Box::new(callback));
        libc::signal(libc::SIGSEGV, breakdown_signal_handler as _);
    }
}

/// Whether a software GL driver is installed.
///
/// Vendor forks of mesa ship their modules as `*_vndri.so` and their loader then
/// refuses the upstream `*_dri.so` spelling, so a machine can have `swrast_dri.so`
/// sitting in the driver directory and still fail to load it. Match whichever
/// naming this machine actually uses.
#[cfg(not(debug_assertions))]
#[cfg(target_os = "linux")]
fn software_gl_available() -> bool {
    const DRI_DIRS: [&str; 3] = [
        "/usr/lib/x86_64-linux-gnu/dri",
        "/usr/lib64/dri",
        "/usr/lib/dri",
    ];
    let mut names = Vec::new();
    for dir in DRI_DIRS {
        if let Ok(entries) = std::fs::read_dir(dir) {
            names.extend(
                entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().to_string()),
            );
        }
    }
    let suffix = if names.iter().any(|name| name.ends_with("_vndri.so")) {
        "_vndri.so"
    } else {
        "_dri.so"
    };
    names.iter().any(|name| {
        (name.starts_with("swrast") || name.starts_with("kms_swrast")) && name.ends_with(suffix)
    })
}

#[cfg(not(debug_assertions))]
#[cfg(not(target_os = "linux"))]
fn software_gl_available() -> bool {
    true
}
