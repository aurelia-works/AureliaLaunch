use aurelia_launch_core::{query, Item, MemoryProvider, SearchProvider};

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
        Item { title: "Terminal".into(), subtitle: "/System/Applications".into(), score: 10, kind: App },
        Item { title: "Safari".into(), subtitle: "/Applications".into(), score: 10, kind: App },
        Item { title: "Finder".into(), subtitle: "/System".into(), score: 10, kind: App },
        Item { title: "AureliaHub".into(), subtitle: "sibling checkout".into(), score: 10, kind: App },
    ]
}

#[tauri::command]
fn search(input: String) -> Vec<Item> {
    query(&input, &PlaceholderProviders)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![search])
        // NOTE: window hide on blur + WebView suspend is configured in
        // tauri.conf.json / capabilities so idle stays near zero.
        .run(tauri::generate_context!())
        .expect("failed to run Aurelia Launch");
}
