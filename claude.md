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
├── dialogs.rs   # UI dialogs (post, reply, prompts, messages)
├── auth.rs      # OAuth authentication (local listener + OOB fallback)
├── config.rs    # Configuration persistence (JSON in APPDATA)
├── mastodon.rs  # Mastodon API client (HTTP, status types)
├── network.rs   # Background network thread for async operations
├── speech.rs    # Screen reader / speech synthesis integration
├── streaming.rs # WebSocket streaming for real-time updates
└── timeline.rs  # Timeline state management
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
| `scraper` | HTML parsing for stripping tags from post content |

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

1. On startup, fetch initial timelines via REST API
2. Connect to streaming API for each timeline type
3. Background threads process WebSocket messages
4. Timer polls channels every 100ms to update UI
5. Automatic reconnection with exponential backoff on disconnect

### UI State Model

- UI event handlers enqueue `UiCommand`s and do not mutate `AppState` directly.
- The main timer tick owns `AppState` and drains queued commands plus network/stream responses.
- This keeps state mutations centralized and avoids re-entrant UI borrows.

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

## Current Features

### Timelines
- **Home timeline** - Posts from accounts you follow (opens by default)
- **Notifications** - Mentions, follows, favourites, boosts (opens by default)
- **Local timeline** - Posts from your instance (opens by default)
- **Federated timeline** - Posts from all known instances
- Real-time streaming updates for all timelines
- Switch between open timelines with the timeline selector
- Configurable sort order (newest first or oldest first)
- Toggle between relative and absolute timestamps

### Posting
- New post dialog with live character count in title bar
- Visibility options: Public, Unlisted, Followers only, Mentioned only
- Content warnings (spoiler text)
- Content type selection: Default, Plain text, Markdown, HTML (for instances that support it)
- Media attachments with alt text descriptions
- Polls with configurable options, duration, and multiple-choice setting
- Configurable send behavior: Enter to send (default) or Ctrl+Enter to send

### Replying
- Reply dialog shows original post preview
- Auto-fills @mention of the author
- Inherits content warning from original post (prefixed with "re: ")
- Matches visibility of original post by default
- Same send behavior as posting (Enter or Ctrl+Enter based on settings)

### Interactions
- Favourite/unfavourite posts
- Boost/unboost posts
- Speech feedback for all actions

### Options (Ctrl+O)
- **Enter to send** - When enabled, Enter sends posts; when disabled, Ctrl+Enter sends
- **Show relative timestamps** - Toggle between "2 hours ago" and "2025-01-27 14:30"
- **Show oldest first** - Reverses timeline order to show oldest posts at the top

### Accessibility
- Native wxWidgets controls for screen reader compatibility
- Speech synthesis feedback for actions (posted, favourited, boosted, errors)
- Full keyboard navigation

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+N | New Post |
| Ctrl+R | Reply to all (includes all mentioned users) |
| Ctrl+Shift+R | Reply to author only |
| Ctrl+Shift+F | Favourite/Unfavourite selected post |
| Ctrl+Shift+B | Boost/Unboost selected post |
| Ctrl+P | View profile of selected post's author |
| Ctrl+T | Open timeline of selected post's author |
| Ctrl+U | Open user by username (profile or timeline) |
| Ctrl+L | Open Local Timeline |
| Enter | View thread timeline for selected post |
| Shift+Enter | Open links in selected post |
| Ctrl+O | Open Options dialog |
| Ctrl+Alt+F | Toggle show/hide window (global hotkey) |
| F5 | Refresh current timeline |
| Delete | Close current timeline (except Home) |

## Next Steps

- Multiple account switching
- Thread/conversation view
- Profile viewing
- Search
