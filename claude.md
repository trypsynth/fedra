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
├── main.rs      # Application entry, UI construction, event handling
├── error.rs     # Simplified error handling with anyhow
├── dialogs.rs   # UI dialogs (prompts, messages, errors)
├── auth.rs      # OAuth authentication (local listener + OOB fallback)
├── config.rs    # Configuration persistence (JSON in APPDATA)
├── mastodon.rs  # Mastodon API client (HTTP, status types)
└── streaming.rs # WebSocket streaming for real-time updates
```

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `wxdragon` | Rust bindings to wxWidgets for native UI |
| `reqwest` | HTTP client with blocking mode, form encoding, JSON, rustls |
| `tungstenite` | WebSocket client for streaming API |
| `anyhow` | Ergonomic error handling with context |
| `serde` / `serde_json` | Configuration and API response serialization |
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

### Timeline & Streaming

The app prefers WebSocket streaming over polling to reduce server load:

1. On startup, fetch initial timeline via REST API (`GET /api/v1/timelines/home`)
2. Connect to streaming API (`wss://instance/api/v1/streaming?stream=user`)
3. Background thread processes WebSocket messages
4. Timer polls the channel every 100ms to update UI
5. Automatic reconnection with exponential backoff on disconnect

### Configuration

- Stored in `%APPDATA%\Fedra\config.json` on Windows
- Falls back to current directory if APPDATA unavailable
- Supports multiple accounts with unique IDs

## Build

Always use release builds for faster iteration:

```bash
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

Early development. Features implemented:
- OAuth authentication with local callback listener
- Account persistence in JSON config
- Home timeline display with real-time streaming updates
- Basic posting via menu (Ctrl+N)
- Manual refresh (F5)

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+N | New Post |
| F5 | Refresh Timeline |

## Next Steps

- Expand post dialog with character count and visibility options
- Support multiple accounts switching
- Display post details (CW, boosts, favorites)
- Local and Federated timelines
- Notifications
