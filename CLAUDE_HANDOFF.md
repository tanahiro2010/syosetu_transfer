# Claude Handoff: syosetu_transfer

## Project Goal

`origin_proj` にある Python/Selenium 版の小説投稿サイト移行ツールを、一般配布しやすい Tauri アプリとして作り直す。

Python 版は以下をしていた。

- カクヨムまたは小説家になろうから作品情報と各話本文を取得
- Selenium + ChromeDriver で投稿先サービスにログイン
- 投稿画面へ各話タイトルと本文を自動入力

Tauri 版では、配布先に Python / Selenium / ChromeDriver を要求しない方向へ寄せた。

## Current Implementation

Tauri + React + TypeScript + Rust backend。

Main files:

- `src-tauri/src/lib.rs`
  - Tauri commands
  - Rust で作品データ取得
- `src/App.tsx`
  - React UI
- `src/App.css`
  - UI styling
- `src-tauri/Cargo.toml`
  - Rust dependencies
- `src-tauri/tauri.conf.json`
  - Tauri config
- `README.md`
  - Basic usage/build notes

## What Works Now

### Rust backend

Implemented Tauri commands:

- `fetch_transfer_draft(platform, source)`
  - `platform = "narou"`:
    - accepts ncode or `https://ncode.syosetu.com/...`
    - calls `https://api.syosetu.com/novelapi/api/?ncode=...&out=json`
    - downloads each episode page
    - extracts title and body
  - `platform = "kakuyomu"`:
    - accepts `https://kakuyomu.jp/works/...`
    - finds first episode
    - follows next episode links
    - extracts title and body
- `build_post_url(platform, management_url)`
  - Kakuyomu: appends `/episodes/new`
  - Narou: validates `syosetu.com` and returns the URL as-is

Rust dependencies added:

- `reqwest`
- `scraper`
- `urlencoding`

### React frontend

Implemented UI:

- Select source platform: カクヨム / 小説家になろう
- Select target platform: カクヨム / 小説家になろう
- Enter source URL or ncode
- Fetch episodes
- Show work summary
- Show episode list
- Select episode
- Copy title
- Copy body
- Copy title + body
- Enter target management URL
- Open posting page in default browser
- Copy full fetched JSON as backup

## Important Design Decision

The original Python app did Selenium-based auto-posting.

The current Tauri implementation intentionally does **not** fully automate posting yet, because doing so naively would reintroduce distribution problems:

- browser driver dependency
- browser automation fragility
- login / 2FA handling
- service UI selector breakage

Instead, the app currently provides a safer distribution-friendly workflow:

1. Fetch all episodes inside the Tauri app.
2. Open the posting page in the user's normal logged-in browser.
3. Let the user copy title/body per episode.

This is not feature parity with Python, but it avoids the original "Python app cannot be broadly distributed" issue.

## Verified Commands

These passed:

```bash
bun run build
cargo check --manifest-path src-tauri/Cargo.toml
bun run tauri build -b app
```

Generated app:

```text
src-tauri/target/release/bundle/macos/syosetu_transfer.app
```

This failed:

```bash
bun run tauri build
```

Reason:

- Release binary and `.app` bundle were built successfully.
- Final DMG packaging failed while running Tauri's generated `bundle_dmg.sh`.
- I did not diagnose deeply because `bun run tauri build -b app` succeeds and produces a usable `.app`.

## Notes And Risks

### Site selectors are fragile

The Rust scraping selectors were ported from the Python version and made slightly more tolerant, but both sites can change HTML at any time.

Narou selectors:

- title: `.p-novel__title`, `.novel_subtitle`
- body: `.p-novel__text`, `#novel_honbun`

Kakuyomu selectors:

- work title: `h1[class*='Heading_heading']`, `h1`, `[itemprop='name']`
- author: `.partialGiftWidgetActivityName a`, `a[href*='/users/']`, `[itemprop='author']`
- episode link: `a[href*='/episodes/']`, `a[href*='/works/'][href*='/episodes/']`
- episode title: `.widget-episodeTitle`, `h1`
- episode body: `.widget-episodeBody`, `[class*='episodeBody']`
- next link: `#contentMain-readNextEpisode`

### No rate limiting yet

Python version slept between episode downloads.

Rust version currently fetches sequentially, but without a deliberate delay. Consider adding a short delay between episode requests to be polite and reduce risk of throttling.

### No persistence yet

Fetched data exists only in UI state. User can copy JSON, but the app does not save/load projects yet.

Good next feature:

- save fetched draft to local file
- load draft from local file
- resume transfer after closing app

### No credentials stored

The Tauri version does not store Kakuyomu/Narou credentials. This is intentional in the current approach.

If adding login automation later, prefer a deliberate design:

- avoid plain-text credential storage
- use OS keychain or keep auth in the user's normal browser
- support 2FA/manual checkpoints

## Suggested Next Steps

1. Test with real public works from both services.
2. Add a small delay between episode fetches.
3. Add save/load draft JSON.
4. Improve error messages for selector failures.
5. Investigate DMG packaging failure if DMG distribution is required.
6. Decide whether "true auto-posting" is needed.

If true auto-posting is required, recommended options:

- Prefer service APIs if available and permitted.
- If no API exists, consider Tauri WebView-based assisted posting rather than ChromeDriver.
- Keep automation semi-manual around login, 2FA, preview, and final submit.

## Quick Start For Claude

From project root:

```bash
bun run tauri dev
```

Build app bundle:

```bash
bun run tauri build -b app
```

Check Rust backend:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Frontend build:

```bash
bun run build
```

## User Preference / Context

The user asked in Japanese:

> origin_projあるじゃないですか  
> これPythonで書いたせいで一般に頒布できなくて  
> なのでTauriで改めて作成してください

So the main motivation is distribution. Be careful not to reintroduce Python/Selenium/ChromeDriver unless the user explicitly accepts that tradeoff.

## Update: WebView-based posting (2026-10)

- `src-tauri/src/fetch.rs`: fetching (moved from lib.rs). Adds `interval_ms` delay and `fetch-progress` events.
- `src-tauri/src/poster.rs`: opens a second WebviewWindow (`poster`) on the target site and drives it via `eval_with_callback` polling. No remote IPC capability is granted. Commands: `open_login_window`, `start_posting`, `continue_posting`, `stop_posting`. Emits `post-progress`.
  - Login is manual in the poster window; cookies persist in the app data dir.
  - Modes: `confirm` (fill, user saves, "次の話へ") / `auto` (fill + click save, wait for URL change, 15s timeout => `submitted` = unverified).
  - Selectors are from the Python version (`form_spec`). Not yet verified against live sites.
- Frontend split: `src/pages/TransferPage.tsx`, `src/pages/SettingsPage.tsx`, `src/hooks/useSettings.ts` (tauri-plugin-store, `settings.json`), `src/types.ts`. Kakuyomu-like styling in `src/App.css`.
