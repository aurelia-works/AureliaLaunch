# Aurelia Launch

Open-source, idle-light macOS launcher in the spirit of Alfred and Raycast.
Rust core + thin Tauri shell. macOS first, Windows/Linux later via provider traits.

Clean-room implementation. No Alfred or Raycast code, assets, or trademarks.
Alfred workflow import (`.alfredworkflow` read-only) is interop only.

## M1: launcher shell

- Global hotkey toggle window
- App + file search (macOS `NSMetadataQuery`/`Spotlight` behind `SearchProvider` trait)
- Calculator, web searches, system commands
- Budgets: idle `<50MB` RAM, cold open `<150ms`, no WebView polling when hidden

## Layout

- `crates/core/` — pure Rust: providers, calc, web search, config. No UI, no Tauri. Fully unit-tested.
- `src-tauri/` — Tauri 2 shell: hotkey, window show/hide, suspends WebView on blur, delegates to core.
- `ui/` — static HTML/JS/CSS, no framework. Virtualized list, zero timers when hidden.
- `docs/ARCHITECTURE.md` — trait boundaries for cross-platform later.

## Run

```bash
# core only (no system deps)
cargo test -p aurelia-launch-core

# full app (macOS, requires Xcode CLT + Rust)
npm install
npm run app:dev

# release bundle
npm run app:build
```

## Budgets (enforced in CI)

- `core` zero heavy deps (no tokio full, no webview in core)
- `ui/` total unminified `<100KB`, zero `setInterval` when hidden
- release `.app` size reported on every build (see `scripts/budget.sh`)
