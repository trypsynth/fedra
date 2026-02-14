# Fedra User Manual

[Fedra](https://github.com/trypsynth/fedra) is a native, keyboard-first Mastodon client for Windows.

## System Requirements
- Windows 10 or Windows 11

## Core Features
- Native Windows UI with screen-reader-friendly controls and live announcements.
- Multi-account support, including account switching while preserving per-account timelines.
- Timelines: Home, Notifications, Local, Federated, Direct Messages, Bookmarks, Favorites, User, Hashtag, Thread, and Search timelines.
- Real-time streaming for Home, Notifications, Local, Federated, and Direct timelines.
- Rich post creation and editing with:
  - Visibility (Public, Unlisted, Followers only, Direct)
  - Content warnings
  - Content type (Default, plain text, markdown, HTML)
  - Media attachments with descriptions
  - Poll creation and voting
- Relationship and discovery tools:
  - Open profile/timeline from posts, mentions, and search
  - Follow/unfollow hashtags
  - View users who boosted/favorited posts
  - Search for accounts, hashtags, and posts
- Tray integration and a global hotkey to show/hide the main window.
- Optional update checks at startup plus manual update checks.

## Window Visibility and Tray
- Fedra runs with a tray icon menu:
  - `Show/Hide`
  - `Exit`
- A global hotkey toggles the main window (default: `Ctrl+Alt+F`).
- You can customize the global hotkey in `Options -> General -> Customize Window Hotkey...`.

## Options
Open options with `Ctrl+,`.

### General Tab
- `Use enter to send posts`
- `Always prompt to open links`
- `Use quick action keys in timelines`
- `Check for updates on startup`
- Notifications mode:
  - Classic Windows notifications
  - Sound only
  - Disabled
- `Customize Window Hotkey...` (Ctrl/Alt/Shift/Win modifiers + custom key)

### Timeline Tab
- Autoload posts:
  - Never
  - When reaching the end
  - When navigating past the end
- Posts to fetch when loading more (`1` to `40`)
- Content warning display:
  - Show inline
  - Don't show
  - CW only
- Display name emoji filtering:
  - None
  - Unicode emojis
  - Instance emojis
  - All
- `Show relative timestamps`
- `Show oldest timeline entries first`
- `Always preserve thread order`
- `Customize Default Timelines...`
  - Home and Notifications are always opened
  - Additional startup timelines are configurable

## Keyboard Shortcuts

### Global / App
- `Ctrl+Alt+F`: Show/hide main window (default global hotkey; customizable)
- `F1`: Open help

### Navigation
- `Left Arrow`: Previous timeline
- `Right Arrow`: Next timeline
- `Backspace`: Go back in timeline history
- `Ctrl+1`..`Ctrl+9`: Switch to timeline index 1-9
- `Ctrl+W`: Close current timeline
- `Ctrl+Backspace`: Close current timeline and navigate back
- `Ctrl+[`: Previous account
- `Ctrl+]`: Next account

### Timelines / Discovery
- `Ctrl+T`: Open selected user's timeline
- `Ctrl+U`: Open user by handle
- `Ctrl+/`: Search
- `Ctrl+L`: Open Local timeline
- `Ctrl+D`: Open Direct Messages timeline
- `Ctrl+.`: Load more posts
- `F5`: Refresh current timeline

### Post Actions
- `Ctrl+N`: New post
- `Ctrl+R`: Reply to all mentioned users
- `Ctrl+Shift+R`: Reply to author only
- `Enter`: Open thread / context (or open selected search result)
- `Alt+Enter`: Open links in selected post
- `Ctrl+P`: View profile
- `Ctrl+M`: View mentions
- `Ctrl+H`: View hashtags
- `Ctrl+Shift+O`: Open selected post in browser
- `Ctrl+Shift+C`: Copy selected post text
- `Ctrl+E`: Edit selected post
- `Delete`: Delete selected post
- `Ctrl+V`: Vote in poll
- `Ctrl+Shift+F`: Favorite/unfavorite
- `Ctrl+Shift+K`: Bookmark/unbookmark
- `Ctrl+Shift+B`: Boost/unboost
- `Ctrl+X`: Toggle CW expansion (CW-only mode)

### Account / Settings
- `Ctrl+Alt+A`: Manage accounts
- `Ctrl+Shift+E`: Edit current profile
- `Ctrl+,`: Open options

### Quick Action Keys Mode
- Enable with `Ctrl+Shift+Q`
- Disable with `q`
- Single-key actions while enabled:
  - `c`: New post
  - `r`: Reply to all
  - `Ctrl+R`: Reply to author
  - `f`: Favorite/unfavorite
  - `k`: Bookmark/unbookmark
  - `b`: Boost/unboost
  - `e`: Edit post
  - `t`: User timeline
  - `m`: Mentions
  - `p`: Profile
  - `h`: Hashtags
  - `o`: Open in browser
  - `v`: Vote
  - `x`: Toggle CW expansion (CW-only mode)
  - `.`: Load more
  - `/`: Search
  - `1`..`9`: Switch timeline index

## Search
- Use `Ctrl+/` to open Search.
- Search types:
  - All
  - Accounts
  - Hashtags
  - Posts
- Results open in a dedicated timeline (`Search: <query>`) and support paging.

## Configuration File
- Installed build: `%APPDATA%\Fedra\config.json`
- Portable/uninstalled run: `config.json` next to the executable

## Changelog

### Version 0.1.0
* Initial release of the Fedra desktop Mastodon client, currently for Windows only.
