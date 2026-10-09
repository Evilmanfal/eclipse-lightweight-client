#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod assets;
mod backend;
mod calls;
mod community;
mod preferences;
mod spotify;
mod sounds;
mod folders;
mod media_picker;
mod message_media;
mod model;
mod identity;
mod timeline;
mod people;
mod presence;
mod ui;
mod navigation;
mod hotkey;
mod settings_layout;
mod widgets;
mod zoom;
mod clipboard;
mod voice_roster;
mod message_time;

use eframe::egui;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|s| s == "--animation-check") {
        if let (Some(input),Some(output)) = (args.get(index+1),args.get(index+2)) {
            let _=std::fs::write(output,serde_json::to_vec_pretty(&assets::animation_check(input)).unwrap());
        }
        return Ok(());
    }
    if let Some(index) = args.iter().position(|s| s == "--icon-check") {
        if let Some(path) = args.get(index + 1) {
            let _ = std::fs::write(
                path,
                serde_json::to_vec_pretty(&assets::network_check()).unwrap(),
            );
        }
        return Ok(());
    }
    if let Some(index) = args.iter().position(|s| s == "--network-check") {
        if let Some(path) = args.get(index + 1) {
            let result = backend::network_check();
            let _ = std::fs::write(path, serde_json::to_vec_pretty(&result).unwrap());
        }
        return Ok(());
    }
    let smoke = args
        .iter()
        .position(|s| s == "--smoke-test")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let preview = args.iter().any(|s| s == "--preview")
        || (smoke.is_some() && !args.iter().any(|s| s == "--smoke-login"));
    let stress = args.iter().any(|s| s == "--stress-preview");
    let collapsed = args.iter().any(|s| s == "--collapsed-folders");
    let dms = args.iter().any(|s| s == "--preview-dms");
    let section=args.iter().position(|s|s=="--preview-section").and_then(|i|args.get(i+1)).cloned();
    set_app_identity();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Eclipse · Native Discord client")
            .with_inner_size(if args.iter().any(|s|s=="--small-window"){[1080.,680.]}else{[1440.,900.]})
            .with_min_inner_size([1080.0, 680.0])
            .with_icon(icon()),
        renderer: eframe::Renderer::Glow,
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "Eclipse",
        options,
        Box::new(move |cc| {
            let mut app = ui::Eclipse::new(cc, preview, smoke);
            if stress {
                app.stress_preview(&cc.egui_ctx);
            }
            app.preview_options(collapsed, dms);
            if let Some(section)=&section {app.preview_section(section);}
            Ok(Box::new(app))
        }),
    )
}

fn icon() -> egui::IconData {
    let image=image::load_from_memory(include_bytes!("../assets/eclipse-icon.png"))
        .expect("bundled Eclipse icon").into_rgba8();
    egui::IconData { width:image.width(),height:image.height(),rgba:image.into_raw() }
}

#[cfg(target_os="windows")]
fn set_app_identity() {
    #[link(name="shell32")]
    extern "system" { fn SetCurrentProcessExplicitAppUserModelID(id:*const u16)->i32; }
    let id:Vec<u16>="Eclipse.Native.Client".encode_utf16().chain(std::iter::once(0)).collect();
    // Give Eclipse a stable taskbar identity, separate from other Discord clients.
    unsafe { SetCurrentProcessExplicitAppUserModelID(id.as_ptr()); }
}
#[cfg(not(target_os="windows"))]
fn set_app_identity() {}

#[derive(Default, Clone, Copy)]
pub struct Memory {
    pub working_mb: f64,
    pub private_mb: f64,
    pub peak_mb: f64,
}
pub fn memory() -> Memory {
    use windows_sys::Win32::System::{
        ProcessStatus::{
            K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
        },
        Threading::GetCurrentProcess,
    };
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        if K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
            counters.cb,
        ) != 0
        {
            Memory {
                working_mb: counters.WorkingSetSize as f64 / 1048576.0,
                private_mb: counters.PrivateUsage as f64 / 1048576.0,
                peak_mb: counters.PeakWorkingSetSize as f64 / 1048576.0,
            }
        } else {
            Memory::default()
        }
    }
}
