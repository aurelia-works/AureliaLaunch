//! Native macOS backends for Aurelia Launch.
//!
//! std-only (no third-party deps): app search walks the well-known
//! application directories, file search delegates to the Spotlight
//! index via `/usr/bin/mdfind`, launching goes through `/usr/bin/open`,
//! and system commands use `pmset` / `CGSession` / AppleScript.
//! Other platforms implement `SearchProvider` separately; see `docs/ARCHITECTURE.md`.

use aurelia_launch_core::{Item, ItemKind, SearchProvider};
use std::path::{Path, PathBuf};
use std::process::Command;

const APP_DIRS: &[&str] = &[
    "/Applications",
    "/System/Applications",
    "/System/Applications/Utilities",
    "/System/Library/CoreServices/Applications",
    "/Library/Application Support",
];

const MAX_WALK_DEPTH: usize = 4;

/// Spotlight-backed provider: installed apps first, then indexed files.
pub struct MacOSProvider;

impl SearchProvider for MacOSProvider {
    fn query(&self, input: &str, limit: usize) -> Vec<Item> {
        if input.trim().is_empty() || limit == 0 {
            return vec![];
        }
        let mut items = search_apps(input, limit);
        if items.len() < limit {
            items.extend(search_files(input, limit - items.len()));
        }
        items.truncate(limit);
        items
    }
}

fn score_name(name: &str, needle: &str) -> Option<i32> {
    let n = name.to_lowercase();
    if n == needle {
        Some(100)
    } else if n.strip_suffix(".app").unwrap_or(&n) == needle {
        Some(100)
    } else if n.starts_with(needle) {
        Some(50)
    } else if n.contains(needle) {
        Some(10)
    } else {
        None
    }
}

/// Scan the application directories for `.app` bundles matching the query.
pub fn search_apps(input: &str, limit: usize) -> Vec<Item> {
    let needle = input.to_lowercase();
    let mut dirs: Vec<PathBuf> = APP_DIRS.iter().map(PathBuf::from).collect();
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(Path::new(&home).join("Applications"));
    }
    let mut hits: Vec<Item> = Vec::new();
    for dir in dirs {
        walk_apps(&dir, &needle, 0, &mut hits, limit.max(20));
        if hits.len() >= limit.max(20) {
            break;
        }
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    hits.truncate(limit);
    hits
}

fn walk_apps(dir: &Path, needle: &str, depth: usize, out: &mut Vec<Item>, cap: usize) {
    if out.len() >= cap || depth > MAX_WALK_DEPTH {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if out.len() >= cap {
            return;
        }
        let path = entry.path();
        if path.extension().map(|e| e == "app").unwrap_or(false) {
            // Resolve symlinks here: stock apps (e.g. Safari) are Cryptex
            // symlinks into /System. Safe: bundles are recorded, never descended into.
            let is_dir = std::fs::metadata(&path)
                .map(|m| m.is_dir())
                .unwrap_or(false);
            if !is_dir {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            if let Some(score) = score_name(&name, needle) {
                let parent = path
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("")
                    .to_string();
                out.push(Item {
                    title: name,
                    subtitle: parent,
                    target: path.to_string_lossy().into_owned(),
                    score,
                    kind: ItemKind::App,
                });
            }
        } else {
            // Descend only into real directories, never symlinks:
            // avoids cycles and escaping the app dirs.
            let is_real_dir = std::fs::symlink_metadata(&path)
                .map(|m| m.is_dir() && !m.is_symlink())
                .unwrap_or(false);
            if is_real_dir {
                walk_apps(&path, needle, depth + 1, out, cap);
            }
        }
    }
}

/// Search the Spotlight index by filename. Whole-disk `mdfind -name` is fast
/// (indexed); output is capped and system noise is filtered out.
pub fn search_files(input: &str, limit: usize) -> Vec<Item> {
    let needle = input.trim().to_lowercase();
    if needle.is_empty() || limit == 0 {
        return vec![];
    }
    let out = match Command::new("/usr/bin/mdfind")
        .arg("-name")
        .arg(&needle)
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut hits = Vec::new();
    for line in text.lines() {
        if hits.len() >= limit * 3 {
            break; // oversample, then rank down to `limit`
        }
        if line.is_empty() || is_noise(line) {
            continue;
        }
        let title = line.rsplit('/').next().unwrap_or(line).to_string();
        let Some(score) = score_name(&title, &needle) else {
            continue;
        };
        let parent = Path::new(line)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();
        hits.push(Item {
            title,
            subtitle: parent,
            target: line.to_string(),
            score: score - 5, // files rank just below same-quality app hits
            kind: ItemKind::File,
        });
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    hits.truncate(limit);
    hits
}

fn is_noise(path: &str) -> bool {
    path.starts_with("/System/")
        || path.starts_with("/private/")
        || path.starts_with("/usr/")
        || path.starts_with("/bin/")
        || path.starts_with("/sbin/")
        || path.starts_with("/dev/")
        || path.contains("/Library/Caches/")
        || path.contains("/.Trash/")
}

/// Open an app bundle or file with the default handler (`open`).
pub fn launch(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("nothing to open".into());
    }
    // Refuse to open things that are not apps or regular files.
    let meta = std::fs::metadata(path).map_err(|e| format!("cannot open {path}: {e}"))?;
    if !meta.is_dir() && !meta.is_file() {
        return Err(format!("cannot open {path}: not a file or app"));
    }
    Command::new("/usr/bin/open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Run one of the built-in system commands by id (see core `system_commands`).
pub fn run_system_command(name: &str) -> Result<(), String> {
    match name {
        "lock" => Command::new(
            "/System/Library/CoreServices/Menu Extras/User.menu/Contents/Resources/CGSession",
        )
        .arg("-suspend")
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string()),
        "sleep" => Command::new("/usr/bin/pmset")
            .arg("sleepnow")
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "emptytrash" => Command::new("/usr/bin/osascript")
            .args(["-e", "tell application \"Finder\" to empty trash"])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "restart" => Command::new("/usr/bin/osascript")
            .args(["-e", "tell application \"System Events\" to restart"])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "shutdown" => Command::new("/usr/bin/osascript")
            .args(["-e", "tell application \"System Events\" to shut down"])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string()),
        other => Err(format!("unknown system command: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_real_apps() {
        // Self-validating: take whatever .app bundles /Applications has
        // (stock apps may be Cryptex symlinks) and require the scan to find one.
        let mut probe: Option<String> = None;
        if let Ok(entries) = std::fs::read_dir("/Applications") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "app").unwrap_or(false)
                    && std::fs::metadata(&path)
                        .map(|m| m.is_dir())
                        .unwrap_or(false)
                {
                    probe = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string());
                    break;
                }
            }
        }
        let name = match probe {
            Some(n) => n,
            None => return, // no /Applications here (e.g. CI Linux): skip
        };
        let hits = search_apps(&name, 5);
        assert!(
            hits.iter().any(|i| i.title == name && i.kind == ItemKind::App),
            "scan should find /Applications/{name}.app"
        );
        assert!(hits.iter().all(|i| !i.target.is_empty()));
    }

    #[test]
    fn empty_query_returns_nothing() {
        assert!(MacOSProvider.query("", 8).is_empty());
        assert!(search_files("   ", 8).is_empty());
    }

    #[test]
    fn rejects_unknown_system_commands() {
        assert!(run_system_command("format-drive").is_err());
    }
}
