# Contributing to SupaCodex

SupaCodex is a native Rust desktop app built with GTK4 and libadwaita.

## Setup

Prerequisites:

- Rust stable
- GTK4 development libraries
- libadwaita development libraries
- `codex` on `PATH` when testing Codex conversations

Run the app and checks:

```bash
npm run dev
npm run lint
npm run test
npm run build
```

Equivalent Cargo commands:

```bash
cargo run --manifest-path native/Cargo.toml
cargo check --manifest-path native/Cargo.toml
cargo test --manifest-path native/Cargo.toml
cargo build --manifest-path native/Cargo.toml --release
```

## Guidelines

- Keep visible UI aligned with GTK/libadwaita conventions.
- Prefer native widgets and CSS classes over custom drawing.
- Keep transparent surfaces transparent so external desktop blur/theme rules can apply.
- Keep changes scoped and update docs when behavior or setup changes.
- Include screenshots or recordings for visible UI changes.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](./LICENSE).
