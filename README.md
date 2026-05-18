# Vivaldi Sync

Sync UI settings, themes, keyboard shortcuts, search engines, and extensions between [Vivaldi](https://vivaldi.com) profiles — locally, without cloud.

![macOS](https://img.shields.io/badge/macOS-000000?logo=apple&logoColor=white)
![Tauri 2](https://img.shields.io/badge/Tauri_2-FFC131?logo=tauri&logoColor=black)
![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)

## What it does

Vivaldi stores each browser profile as a separate folder with a `Preferences` JSON file. This app reads that file from a source profile and copies specific keys to a destination profile — without touching your bookmarks, history, or passwords.

## Features

- **Sync settings** — tab bar position, toolbar layout, themes, keyboard shortcuts, search engine, panels, and more
- **Copy extensions** — copy installed extensions from one profile to another
- **Granular control** — basic mode (by category) or advanced mode (individual keys)
- **Auto-sync** — run on app launch or on a schedule (every 5 min to 2 hours)
- **Dry run** — preview what would be copied before committing
- **Menu bar app** — runs in the background as a tray icon; window hides instead of closing
- **Start at login** — optional macOS login item

## What gets synced

| Category | What's included |
|----------|----------------|
| 🖥️ UI Layout | Tab bar position, close button side, toolbar buttons, address bar, auto-hide rules |
| 🎨 Appearance & Themes | Active theme, saved custom themes, window accent color |
| ⌨️ Keyboard Shortcuts | Custom key bindings, command chains, action definitions |
| ☰ Menus & Context | Menu bar customization, right-click context menu items |
| 📄 Page & Content | Default zoom, font rendering, per-site overrides, translation |
| 🔍 Search & Address Bar | Default search engine, search suggestions, search nicknames |
| 📌 Panels & Workspaces | Web panels, workspaces, sidebar settings |
| 🚀 Startup & New Tab | Startup page, new tab behavior, homepage |
| 🛡️ Privacy & Passwords | Tracker blocking, cookie settings (not passwords) |
| ⚙️ Advanced Settings | Flags and experimental features |

## Installation

Download the latest `.dmg` from [Releases](https://github.com/smithplus/VivaldiProfileSyncer/releases), open it, and drag **Vivaldi Sync** to your Applications folder.

Or build from source:

```bash
# Prerequisites: Rust, Node.js
git clone https://github.com/smithplus/VivaldiProfileSyncer
cd VivaldiProfileSyncer
npm install
npm run tauri build
```

The built app will be at `src-tauri/target/release/bundle/macos/Vivaldi Sync.app`.

## Usage

1. **Close Vivaldi** before syncing — Vivaldi holds its Preferences file in memory and will overwrite any changes when it exits.
2. Select a **FROM** and **TO** profile.
3. Choose what to sync (or use Advanced mode for fine-grained control).
4. Click **Dry Run** to preview, then **Sync Now** to apply.

> Settings are saved automatically. Auto-sync will only copy the keys you explicitly approved — never defaults to syncing everything.

## Safety

- Only touches the `Preferences` file — bookmarks, history, passwords, and extensions data are never modified during a settings sync.
- Creates a `.bak` backup of the destination `Preferences` before every write.
- The app warns you if Vivaldi is running and disables the Sync button.

## Built with

- [Tauri 2](https://tauri.app) — Rust backend + WebView frontend
- Rust — file I/O, profile discovery, extension copying
- Vanilla JS — no framework

## License

MIT © [Martin Smith](https://github.com/smithplus)

## Feedback

Found a bug or have a feature request? [Open an issue](https://github.com/smithplus/VivaldiProfileSyncer/issues).

## Support

If this saved you time, you can [buy me a coffee ☕](https://buymeacoffee.com/smithplus) — it's appreciated but never required.
