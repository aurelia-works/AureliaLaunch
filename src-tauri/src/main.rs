use aurelia_launch_core::{query, Item, ItemKind};
#[cfg(not(target_os = "macos"))]
use aurelia_launch_core::{MemoryProvider, SearchProvider};
use serde::Serialize;
use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

// Toggle hotkey. Option+Space avoids fighting Spotlight (Cmd+Space)
// and is reachable without moving your hands off home row.
const TOGGLE_SHORTCUT: &str = "Alt+Space";

/// Wire shape for the UI. Core stays serde-free; the shell owns serialization.
#[derive(Serialize)]
struct Hit {
    title: String,
    subtitle: String,
    /// What acting on the row needs: path (App/File), value (Calc),
    /// URL (WebSearch) or command id (SystemCommand).
    target: String,
    kind: &'static str,
}

fn kind_name(k: ItemKind) -> &'static str {
    match k {
        ItemKind::App => "App",
        ItemKind::File => "File",
        ItemKind::Calc => "Calc",
        ItemKind::WebSearch => "WebSearch",
        ItemKind::SystemCommand => "SystemCommand",
    }
}

fn to_hit(i: Item) -> Hit {
    Hit {
        title: i.title,
        subtitle: i.subtitle,
        target: i.target,
        kind: kind_name(i.kind),
    }
}

#[cfg(target_os = "macos")]
fn backend_query(input: &str) -> Vec<Item> {
    query(
        input,
        &aurelia_launch_platform_macos::MacOSProvider,
    )
}

/// Non-macOS (and fallback): a few well-known entries so the UI stays usable.
#[cfg(not(target_os = "macos"))]
fn backend_query(input: &str) -> Vec<Item> {
    let mem = MemoryProvider {
        entries: vec![
            Item {
                title: "Terminal".into(),
                subtitle: "fallback entry (native backend is macOS-only for now)".into(),
                target: String::new(),
                score: 10,
                kind: ItemKind::App,
            },
        ],
    };
    query(input, &mem)
}

#[tauri::command]
fn search(input: String) -> Vec<Hit> {
    backend_query(&input).into_iter().map(to_hit).collect()
}

/// Open an app bundle or file with its default handler.
#[tauri::command]
fn launch(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return aurelia_launch_platform_macos::launch(&path);
    #[cfg(not(target_os = "macos"))]
    return Err(format!("launching is macOS-only for now: {path}"));
}

/// Run a built-in system command by id (`lock`, `sleep`, ...).
#[tauri::command]
fn run_system(name: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return aurelia_launch_platform_macos::run_system_command(&name);
    #[cfg(not(target_os = "macos"))]
    return Err(format!("system commands are macOS-only for now: {name}"));
}

/// Open an http(s) URL in the default browser. Other schemes are refused.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("only http(s) URLs can be opened".into());
    }
    std::process::Command::new("/usr/bin/open")
        .arg(&url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Copy text to the macOS pasteboard via pbcopy (no clipboard plugin needed).
#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .ok_or("pbcopy stdin unavailable")?
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    child.wait().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn hide_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn toggle_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_main(app);
        }
    }
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcut(TOGGLE_SHORTCUT)
                .expect("toggle shortcut must parse")
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_main(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            search,
            launch,
            run_system,
            open_url,
            copy_text,
            hide_window
        ])
        .on_window_event(|window, event| {
            // Hide on blur: a hidden window does no layout or paint work.
            if let WindowEvent::Focused(false) = event {
                let _ = window.hide();
            }
        })
        .setup(|app| {
            show_main(app.handle());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Aurelia Launch");

    app.run(|handle, event| {
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen { .. } = event {
            show_main(handle);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (handle, event);
    });
}
