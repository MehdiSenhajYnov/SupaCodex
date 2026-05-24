# SupaCodex

SupaCodex is a local-first GTK4/libadwaita desktop app for AI-assisted coding.

The app is built as a native Rust application. Its UI uses GTK4 and libadwaita directly so it follows the host desktop theme, including compositor-provided transparency and blur effects when your system theme enables them.

## Features

- Native libadwaita window, header bar, sidebar, thread list, chat view, and composer
- Transparent GTK surfaces designed to inherit system blur/transparency from the desktop theme
- Workspace/project discovery backed by the existing SQLite data model
- Codex and Claude engine orchestration through the Rust backend
- Streaming assistant messages with text, thinking, actions, diffs, notices, errors, and approval blocks
- Per-workspace threads persisted locally in `~/.supacodex`

## Requirements

- Rust stable
- GTK4 development libraries
- libadwaita development libraries
- Git, for workspace repository detection
- `codex` on `PATH` for Codex conversations
- Node.js only if you use the Claude sidecar locally

On GNOME/Fedora-style systems, the native libraries are usually available from the distribution packages for `gtk4-devel` and `libadwaita-devel`. On Debian/Ubuntu-style systems, install the matching `libgtk-4-dev` and `libadwaita-1-dev` packages.

## Development

```bash
npm run dev
npm run lint
npm run test
npm run build
```

The npm scripts are thin wrappers around Cargo:

```bash
cargo run --manifest-path native/Cargo.toml
cargo check --manifest-path native/Cargo.toml
cargo test --manifest-path native/Cargo.toml
cargo build --manifest-path native/Cargo.toml --release
```

## Runtime Data

| Path | Purpose |
|---|---|
| `~/.supacodex/config.toml` | App configuration |
| `~/.supacodex/workspaces.db` | Workspace, thread, message, and action database |
| `~/.supacodex/logs` | App logs |

## Architecture

The application is intentionally native:

| Layer | Technology |
|---|---|
| UI | GTK4 + libadwaita |
| Language | Rust |
| Persistence | SQLite via `rusqlite` |
| Engines | Codex app-server and Claude sidecar |
| Repository support | `git2` plus git CLI fallback helpers |

The GTK window and core surfaces are transparent or semi-transparent rather than painted with an opaque custom shell. That lets your system theme or compositor own blur strength, shadows, and backdrop behavior.
