# Fedra User Manual

[Fedra](https://github.com/trypsynth/fedra) is a native, keyboard-first Mastodon client for Windows.

## System Requirements
Windows 10 or 11

## Core Features
- Native Windows UI with screen-reader-friendly controls and live announcements.
- Multi-account support, including account switching while preserving per-account timelines.
- Timelines: Home, Notifications, Local, Federated, Direct Messages, Bookmarks, Favorites, User, Hashtag, Thread, and Search timelines.
- Real-time streaming for Home, Notifications, Local, Federated, and Direct timelines.
- Rich post creation and editing with:
  - Visibility (Public, Unlisted, Followers only, Direct)
  - Content warnings
  - Content type (Default, plain text, markdown, HTML)
  - Optional post language (ISO code)
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
- `Show oldest timeline entries first`
- `Always preserve thread order`
- `Customize Default Timelines...`
  - Home and Notifications are always opened
  - Additional startup timelines are configurable
- Post language:
  - Per-post ISO language code can be set in compose dialogs

### Templates Tab
Customize how posts appear in each timeline using [Jinja2-style](https://jinja.palletsprojects.com/en/stable/templates/) templates.

- Select a timeline from the dropdown (or **Global Default** to set the fallback used by all timelines without their own override).
- Edit the **Post template**, **Boost template**, and **Quote template** text fields.
- Click **Reset to default** to restore the selected timeline's templates to the global default (or restore the global default to the built-in default).

Templates are rendered per-entry each time a timeline is displayed. If a template contains a syntax error, the entry falls back to `author: content`.

#### Available Variables

| Variable | Value |
|---|---|
| `{{ author }}` | Display name (respects emoji filtering setting) |
| `{{ username }}` | `@acct` handle |
| `{{ content }}` | Post text, HTML-stripped (respects content warning display setting) |
| `{{ content_warning }}` | Spoiler text, or empty if none |
| `{{ relative_time }}` | Relative timestamp, e.g. `2 hours ago` |
| `{{ absolute_time }}` | Absolute local timestamp, e.g. `Feb 17, 2026 at 2:30 PM` |
| `{{ visibility }}` | `Public`, `Unlisted`, `Followers only`, or `Direct` |
| `{{ reply_count }}` | e.g. `3 replies` |
| `{{ boost_count }}` | e.g. `1 boost` |
| `{{ favorite_count }}` | e.g. `5 favorites` |
| `{{ client }}` | Posting app name, or empty if unknown |
| `{{ media }}` | Media attachment summary, or empty if none |
| `{{ poll }}` | Poll summary, or empty if none |
| `{{ booster }}` | Display name of the person who boosted (boost template only; empty for regular posts) |
| `{{ booster_username }}` | `@acct` handle of the booster (boost template only) |
| `{{ quote_author }}` | Display name of the quoted post's author (quote/boost templates) |
| `{{ quote_username }}` | `@acct` handle of the quoted post's author (quote/boost templates) |
| `{{ quote_content }}` | Text content of the quoted post (quote/boost templates) |
| `{{ quote_media }}` | Media summary of the quoted post (quote/boost templates) |
| `{{ quote_poll }}` | Poll summary of the quoted post (quote/boost templates) |

#### Conditionals

You can use `{% if %}` blocks to show text only when a variable is non-empty:

```
{% if client %}, via {{ client }}{% endif %}
```

### Filters Tab
Hide post types per timeline. Select a timeline from the dropdown, then check the types you want to hide:
- Original posts (not replies or boosts)
- Replies to others
- Replies to me
- Threads (self-replies)
- Boosts
- Quote posts
- Posts with media
- Posts without media
- Your posts
- Your replies

## Keyboard Shortcuts

### Global / App
- `Ctrl+Alt+F`: Show/hide main window (default global hotkey; customizable)
- `F1`: Open help

### Navigation
- `Left Arrow`: Previous timeline
- `Right Arrow`: Next timeline
- `Ctrl+1`..`Ctrl+9`: Switch to timeline index 1-9
- `Ctrl+W`: Close current timeline (when Quick Action Keys are off)
- `Delete` (in Timelines list): Close current timeline
- `Ctrl+[`: Previous account
- `Ctrl+]`: Next account
- `Ctrl+Shift+Left Arrow`: Move current timeline left (in Timelines list)
- `Ctrl+Shift+Right Arrow`: Move current timeline right (in Timelines list)
- `Shift+Left Arrow`: Move current timeline left (in Posts list)
- `Shift+Right Arrow`: Move current timeline right (in Posts list)

### Timelines / Discovery
- `Ctrl+T`: Open selected user's timeline
- `Ctrl+U`: Open user by handle
- `Ctrl+/`: Search
- `Ctrl+Shift+I`: Open an instance's local timeline by domain
- `Ctrl+L`: Open Local timeline
- `Ctrl+D`: Open Direct Messages timeline
- `Ctrl+Shift+M`: Open Mentions timeline
- `Ctrl+.`: Load more posts
- `F5`: Refresh current timeline
- `Ctrl+F`: Find in current timeline
- `F3`: Find next occurrence
- `Shift+F3`: Find previous occurrence

### Post Actions
- `Ctrl+N`: New post
- `Ctrl+R`: Reply to all mentioned users
- `Ctrl+Shift+R`: Reply to author only
- `Ctrl+Q`: Quote selected post
- `Enter`: Open thread / context (or open selected search result)
- `Shift+Enter`: View post details in Fedra
- `Alt+Enter`: Open links in selected post
- `Ctrl+I`: Play media attached to selected post
- `Ctrl+P`: View profile
- `Alt+F`: Follow/unfollow the post's author
- `Ctrl+M`: View mentions
- `Ctrl+H`: View hashtags
- `Ctrl+Shift+O`: Open selected post in browser
- `Ctrl+Shift+C`: Copy selected post text
- `Ctrl+E`: Edit selected post
- `Delete` (in Posts list): Delete selected post
- `Ctrl+V`: Vote in poll
- `Ctrl+Shift+F`: Favorite/unfavorite
- `Ctrl+Shift+K`: Bookmark/unbookmark
- `Ctrl+Shift+B`: Boost/unboost
- `Ctrl+X`: Toggle CW expansion (CW-only mode)

### Account / Settings
- `Ctrl+Alt+A`: Manage accounts
- `Ctrl+Shift+E`: Edit current profile
- `Ctrl+,`: Open options
- Options menu → **Manage Filters...**: View, add, edit, or delete server-side content filters
- Options menu → **Manage Lists...**: Create and manage Mastodon lists; open a list timeline via Timelines → Open List...

### Quick Action Keys Mode
- Toggle with `Ctrl+Shift+Q`
- Single-key actions while enabled:
  - `Backspace`: Close current timeline
  - `c`: New post
  - `r`: Reply to all
  - `Ctrl+R`: Reply to author
  - `q`: Quote post
  - `f`: Favorite/unfavorite
  - `k`: Bookmark/unbookmark
  - `b`: Boost/unboost
  - `e`: Edit post
  - `i`: Play media
  - `t`: User timeline
  - `u`: Open user by handle
  - `Shift+I`: Open instance timeline by domain
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

### Version 0.3.1
* Added a mentions timeline.
* Added an option to open the local timeline for a spisific instence.
* Added the ability to play back and download media in posts!
* Fixed quick keys not disabling properly until you changed your list position.
* You will no longer get a select user dialog with two of the same entry for posts where a user boosts their own post.
* Your last-viewed post is now automatically restored upon relaunch if it is successfully fetched.

### Version 0.3.0
* Added an option to hide the totals from polls, and switched to preset amounts of time for poll durations.
* Added an option to restore previously opened timelines on startup.
* Added an option to show link previews in the timeline.
* Added many more supported extensions to the add media dialog.
* Added support for managing and opening list timelines.
* Added support for reading and writing quote posts.
* Added a new timeline filters tab to the options dialog, allowing you to filter your timelines on the client side.
* Added the ability to schedule posts.
* Added the {{ booster_username }} template variable for consistent @username display.
* Added a thread mode check box to the new post dialog. When checked, every time you hit post, you'll get the dialog again, and be replying to your previous post.
* Fedra will now respect the account-wide default post visibility in the new post dialog.
* Fixed message duplication in the direct messages timeline.
* Fixed the description fields in the add media dialog not showing up.
* Fixed the post context menu not showing hotkeys and post-specific actions such as edit or delete.
* Fixed the post context menu showing incorrect labels for actions on boosted/favorited posts.
* It is now possible to interact with follower requests.
* It is now possible to reorder your timelines with ctrl+shift+left/right arrow.
* It is now possible to search your timelines with ctrl+f and f3/shift+f3.
* List timelines now stream.
* Opening the select user dialog is now much more responsive.
* Pinned posts are now shown at the top of user timelines.
* Removed the buggy global template system for now. There are plans to rewrite it in a much more stable way in the future.
* The default templates now hide the reply/boost/favorite counts if they're zero.
* The post details webview will now come up much faster and smoother.
* The timeline switching hotkeys now work in the list of timelines as well as the timeline list.
* You can now pin/unpin posts.

### Version 0.2.0
* Added a webview-based dialog for viewing the raw contents of a post.
* Added a new option, checked by default, to remove tracking parameters from URLs.
* Added an option to check for development builds upon update, not new stable releases.
* Added timeline templates, allowing you to customize everything about how Fedra's timeline entries are displayed. The relative/absolute time check box has also been removed from the options dialog, and is now settable per-template. See the templates section of the readme for more details.
* Filters are now respected in the timeline, and you can manage them in a super basic sense. This capability will be expanded in a future version.
* Fixed attaching media, so more than teeny tiny files work now.
* Fixed the handling of JSON responses from certain servers.
* Hopefully fixed a rare but annoying crash in the new post dialog.
* It is now possible to type past the character limit once again, but you will get a warning sound when you do so.
* Post statistics are now properly pluralized, so you will now hear "1 reply" instead of "1 replies".
* pressing shift+f10 or the context menu key on a post will now bring up a menu of post actions.
* Replies are properly grouped in threads now.
* The  mentions dialog will now include users who haven't fedrated with your instance yet.
* The open user dialog will now be automatically prepopulated with all of the usernames that appear in your current timeline.
* When closing a timeline, the name of the newly focused one will now be spoken before the timeline contents, as intended.

### Version 0.1.1
* Added the ability for you to set the language of your posts!
* API errors are now included in error output in a brief form.
* Fixed Delete not closing timelines when the list had keyboard focus.
* Improved default configuration values for new Fedra installs.
* Reduced unnecessary screen reader output when entering the compose dialog.
* The compose dialog now enforces the instance's character limit.
* The focused timeline name is now spoken when using Ctrl+1-9, matching left/right arrow behavior.
* The reply dialog title is now announced with the correct character count on first focus.
* There is now only one key to back out of a timeline and close it, Ctrl+W normally or backspace in quick keys mode.
* Updated the README and performed internal code cleanup.
* When a timeline is closed, the newly focused timeline is now announced.

### Version 0.1.0
* Initial release of the Fedra desktop Mastodon client, currently for Windows only.
