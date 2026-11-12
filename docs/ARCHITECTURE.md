# Aurelia Launch architecture

## Goal
Alfred/Raycast-class launcher, open-source, idle-light (<50MB), macOS first.

## Boundaries
- `crates/core` — pure Rust. Query pipeline: `SearchProvider -> calc -> web -> system -> rank`.
  No Tauri, no AppKit, no network. Unit tests enforce ranking and calc.
- `platform` (future crate) — `trait SearchProvider` impls:
  - macOS: Spotlight `MDQuery` / `NSMetadataQuery`, `/Applications` scan, Quick Look
  - Windows: Everything SDK / Start menu index (later)
  - Linux: `locate` / tracker / `.desktop` scan (later)
- `src-tauri` — thin shell: global hotkey, show/hide window, `search` command,
  open/copy actions, hide-on-blur. No ranking logic here.
- `ui` — static, framework-free. Event-driven only: no `setInterval`/`requestAnimationFrame`
  loops, max 20 DOM rows. Idle cost must be ~zero when hidden.

## Idle budget enforcement
- `scripts/budget.sh` fails CI if `ui/` exceeds 100KB or contains timers.
- `cargo bloat`-style check: core must stay dependency-light (CI lists `cargo tree`).
- Release `.app` size is reported each build; alert threshold 30MB compressed.

## Extensions (M3, not M1)
- JS sandbox commands via Tauri `invoke` allowlist.
- Store = signed Git repos + manifest, modeled on Raycast store UX.
- Alfred `.alfredworkflow` import is read-only interop (parse plist/JSON, map
  keyword+script-filter to Launch commands). No Alfred code reused.
