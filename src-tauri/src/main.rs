use aurelia_launch_core::{query, Item, ItemKind, MemoryProvider, SearchProvider};
use serde::Serialize;
use tauri::{Manager, RunEvent, WindowEvent};

// Placeholder provider until the macOS Spotlight (MDQuery) backend lands.
// Kept behind the SearchProvider trait so Windows/Linux backends slot in later.
struct PlaceholderProviders;

impl SearchProvider for PlaceholderProviders {
    fn query(&self, input: &str, limit: usize) -> Vec<Item> {
        let mem = MemoryProvider { entries: seed_entries() };
        mem.query(input, limit)
    }
}

fn seed_entries() -> Vec<Item> {
    use aurelia_launch_core::ItemKind::*;
    vec![
        Item { title: "Terminal".into(), subtitle: "/System/Applications/Utilities".into(), score: 10, kind: App },
        Item { title: "Safari".into(), subtitle: "/Applications".into(), score: 10, kind: App },
        Item { title: "Finder".into(), subtitle: "/System/Library/CoreServices".into(), score: 10, kind: App },
        Item { title: "AureliaHub".into(), subtitle: "sibling checkout".into(), score: 10, kind: App },
    ]
}

/// Wire shape for the UI. Core stays serde-free; the shell owns serialization.
#[derive(Serialize)]
struct Hit {
    title: String,
    subtitle: String,
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

#[tauri::command]
fn search(input: String) -> Vec<Hit> {
    query(&input, &PlaceholderProviders)
        .into_iter()
        .map(|i| Hit { title: i.title, subtitle: i.subtitle, kind: kind_name(i.kind) })
        .collect()
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

fn main() {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![search, open_url, copy_text, hide_window])
        .on_window_event(|window, event| {
            // Hide on blur: a hidden window does no layout or paint work.
            if let WindowEvent::Focused(false) = event {
                let _ = window.hide();
            }
        })
        .setup(|app| {
            // Until the global hotkey lands, show once at launch; Dock click re-shows.
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
