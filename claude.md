# Fedra

A native Mastodon desktop client for Windows, built with Rust and wxWidgets via wxDragon bindings.

## Project Goals

- **Accessibility**: First-class screen reader and keyboard support via native wxWidgets controls
- **Lightweight**: Minimal resource usage, fast startup, small binary size
- **Native Experience**: Uses the Windows native UI toolkit for proper theming and integration

## Architecture

### Module Structure

```
src/
├── main.rs      # Application entry, UI construction, account setup flow
├── error.rs     # Simplified error handling with anyhow
├── auth.rs      # OAuth authentication (local listener + OOB fallback)
├── config.rs    # Configuration persistence (JSON in APPDATA)
└── mastodon.rs  # Mastodon API client (blocking reqwest)
```

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `wxdragon` | Rust bindings to wxWidgets for native UI |
| `reqwest` | HTTP client with blocking mode, form encoding, JSON, rustls |
| `anyhow` | Ergonomic error handling with context |
| `serde` / `serde_json` | Configuration serialization |
| `url` | URL parsing and manipulation |
| `webbrowser` | Open authorization URLs in default browser |

### Error Handling

Uses `anyhow` for simple, ergonomic error handling:

- `anyhow::Result<T>` as the standard return type
- `.context("message")` to add context to errors
- `error::user_message()` extracts user-friendly messages for UI dialogs

```rust
// Example usage
let listener = TcpListener::bind("127.0.0.1:0")
    .context("Failed to bind OAuth listener")?;
```

### Authentication Flow

1. Try OAuth with local TCP listener (port 0 for auto-assignment)
2. Fall back to out-of-band (OOB) code entry via dialog
3. Fall back to manual access token entry

### Configuration

- Stored in `%APPDATA%\Fedra\config.json` on Windows
- Falls back to current directory if APPDATA unavailable
- Supports multiple accounts with unique IDs

## Build

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run clippy for lints
cargo clippy --release
```

## Code Style

Configured in `rustfmt.toml`:

- Rust Edition 2024
- Tab indentation
- 120 character line limit
- Grouped imports

## Windows Integration

`build.rs` embeds a Windows manifest for:

- DPI awareness (per-monitor v2)
- Common controls v6 for modern theming
- UTF-8 code page

## Current Status

Early development. Basic OAuth flow and account persistence implemented. UI shows timeline selector but does not yet fetch or display posts.

## Next Steps

- Fetch and display home timeline
- Implement posting functionality
- Add keyboard shortcuts
- Support multiple accounts switching
- Timeline refresh and streaming
