# Fedra User Manual

[Fedra](https://github.com/trypsynth/fedra) is a native, keyboard-first Mastodon client for Windows.

## System Requirements
Windows 10 or 11

## Core Features
- Native Windows UI with screen-reader-friendly controls and live announcements, built on a custom [AccessKit](https://accesskit.dev)-backed list control.
- Multi-account support, including account switching while preserving per-account timelines.
- A separate notification sound for each kind of event, from mentions and direct messages to follows, boosts, and favorites. Fedra ships one sound and plays it for everything until you supply your own, one event at a time or a whole pack at once.
- Timelines: Home, Notifications, Mentions, Sent, Local, another instance's Local, Federated, Direct Messages, Bookmarks, Favorites, Lists, User, Hashtag, Thread, and Search.
- Real-time streaming for Home, Notifications, Local, Federated, Direct, and List timelines. Your own posts also appear in the Sent timeline as soon as you publish them.
- Rich post creation and editing with:
  - Visibility (Public, Unlisted, Followers only, Direct)
  - Content warnings
  - Content type (Default, plain text, markdown, HTML)
  - Optional post language (ISO code)
  - Media attachments with descriptions, optionally marked sensitive
  - Polls, with preset durations, multiple choice, and optionally hidden vote counts
  - Quote posts
  - Scheduled posts
  - Thread mode, for writing a chain of self-replies without reopening the dialog
- Relationship and discovery tools:
  - Open profile/timeline from posts, mentions, boost/favorite lists, and search
  - Follow/unfollow, block, mute, show/hide a user's boosts, and add users to lists
  - Accept or reject follow requests
  - Browse a user's followers and following
  - Follow/unfollow and mute hashtags
  - View users who boosted/favorited posts
  - Search for accounts, hashtags, and posts
- Built-in media player with download support.
- Fully customizable keyboard shortcuts, with independent bindings for normal and Quick Action Keys modes.
- Client-side timeline filters, plus management of server-side Mastodon filters.
- Mastodon list management (create, edit, delete, and change membership) and list timelines.
- Customizable timeline entry and window title templates.
- Tray integration and a global hotkey to show/hide the main window.
- Optional update checks at startup plus manual update checks, on either the stable or development channel.

## Main Window Layout
The main window has two lists:

- The **Timelines** list, holding every open timeline in order.
- The **Posts** list, holding the entries of the currently selected timeline.

`Tab` and `Shift+Tab` move between them. Timeline switching, reordering, and closing work from either list; post actions work from the Posts list.

## Timelines

### Opening Timelines
Timelines are opened from the **Timelines** menu, from a post (user timelines, threads, hashtags), or with their shortcut. Every open timeline appears in the Timelines list and stays open until you close it with `Ctrl+W` (or `Backspace` in Quick Action Keys mode).

| Timeline | How to open |
|---|---|
| Home | `Timelines -> Home Timeline` |
| Notifications | `Timelines -> Notifications` |
| Mentions | `Ctrl+Shift+M` |
| Sent | `Timelines -> Sent` |
| Local | `Ctrl+L` |
| Local for another instance | `Ctrl+Shift+I`, then type a domain |
| Federated | `Timelines -> Federated Timeline` |
| Direct Messages | `Ctrl+D` |
| Bookmarks | `Timelines -> Bookmarks` |
| Favorites | `Timelines -> Favorites` |
| List | `Timelines -> Open List...` |
| User | `Ctrl+T` on a post, or `Ctrl+U` to type a handle |
| Hashtag | `Ctrl+H` on a post, then **View Timeline** |
| Thread | `Alt+Enter` on a post |
| Search | `Ctrl+/` |

Home and Notifications are opened automatically at startup, but they are not special: you can close them like any other timeline and reopen them from the Timelines menu.

### The Sent Timeline
`Timelines -> Sent` opens your own account's timeline in a buffer, so you can see everything you have posted, including replies and boosts, with your pinned posts at the top. Posts you publish or delete are reflected there live.

### Reordering and Switching
Reorder timelines with `Shift+Left Arrow` and `Shift+Right Arrow` from either list. Switch between them with `Left Arrow`/`Right Arrow` or `Ctrl+1` through `Ctrl+9`.

### Refreshing
Streaming timelines update themselves. `F5` refreshes the current timeline at any time, and `.` (Load More) fetches older entries. If a streaming timeline loses its connection, Fedra re-fetches it about once a minute until streaming comes back.

## Window Visibility and Tray
- Fedra runs with a tray icon menu:
  - `Show/Hide`
  - `Exit`
- A global hotkey toggles the main window (default: `Ctrl+Alt+F`).
- You can customize the global hotkey in `Options -> General -> Customize Window Hotkey...`.

## Composing Posts
`Ctrl+N` opens the compose dialog; `Ctrl+R`, `Ctrl+Shift+R`, `Ctrl+Q`, and `Ctrl+E` open it for a reply, an author-only reply, a quote, and an edit respectively. Every control has an access key, and the dialog title shows the character count for your instance's limit. You can type past the limit, but you will hear a warning sound when you do.

The dialog offers:

- **Content warning**: a checkbox plus the warning text field.
- **Content type**: Default, plain text, Markdown, or HTML, for instances that support it. Editing a Markdown post gives you back your original Markdown, not the rendered text.
- **Visibility**: Public, Unlisted, Followers only, or Direct. The initial value follows your account's default visibility.
- **Post language**: an ISO code, defaulting to your account's setting.
- **Manage Media...**: add attachments, give each one a description, and mark the set as sensitive.
- **Add Poll...**: add options up to your instance's limit, with a preset duration, optional multiple selections, and an option to hide vote counts until the poll closes.
- **Schedule...**: pick a local date and time to publish at, or **Clear Schedule** to post immediately.
- **Thread mode**: when checked, posting reopens the dialog as a reply to the post you just made, so you can write a thread without leaving the dialog.

If `Use enter to send posts` is enabled, `Enter` posts from the content field; otherwise use the Post button.

## Options
Open options with `Ctrl+,`.

### General Tab
- `Use enter to send posts`
- `Always prompt to open links`
- `Read link previews in timelines`
- `Strip tracking parameters from URLs`
- `Use quick action keys in timelines`
- `Check for updates on startup`
- Update channel:
  - `Stable`
  - `Dev` (development builds)
- Notifications mode:
  - Classic Windows Notifications
  - Sound only
  - Sound and notifications
  - Disabled
- `Customize Keyboard Shortcuts...`
- `Customize Window Hotkey...` (Ctrl/Alt/Shift/Win modifiers + custom key)

### Timeline Tab
- `Restore open timelines on startup` (when off, only your default timelines are reopened)
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
- `Load more on find next`
- `Customize Default Timelines...`
  - Home and Notifications are opened at startup
  - Any of Local, Federated, Direct Messages, Bookmarks, Favorites, Mentions, and Sent can be added

### Sounds Tab
Fedra can play a different sound for each kind of event, and every one of them is a file
of your choosing.

Fedra ships a single sound, `boop.mp3`, and every event falls back to it. A new install
therefore sounds exactly as Fedra always has: one sound for everything. The moment you
give an event a file of its own, that event uses it and the rest carry on with the
fallback. No per-event audio is bundled, deliberately, so nothing here is anyone's taste
but yours.

- `Enable notification sounds`: master switch for every sound below.
- `Sound pack`: the set of sounds to use. Switching packs changes every event at once,
  while leaving anything you unchecked still unchecked. A pack that provides some but not
  all of the sounds says so in the list; the events it leaves out fall back to the default
  pack, and then to `boop.mp3`.
- `Volume`: 0 to 100, applied to all sounds.
- The sound list shows one entry per event, each with the file it currently uses.
  Unchecking an entry silences just that event and leaves the rest alone.

Sounds are tied to actions, not just to things arriving. Where an action has two
directions, both share one sound: favoriting a post and having one of yours favorited
are both the favorite sound. Undoing an action reuses its sound, since the point is to
confirm something happened and the screen reader already says which way it went.

| Event | When it plays |
| --- | --- |
| Mention | Someone mentions you in a public, unlisted, or followers-only post |
| Direct message | Someone mentions you in a direct post |
| New post in home timeline | Somebody you follow posts. The busiest event by far, so give it the shortest and quietest sound you have, or uncheck it to silence just this one |
| Followed someone, or gained a follower | You follow or unfollow an account or hashtag, or somebody follows you |
| Follow request | Somebody asks to follow you, or you accept or reject a request |
| Favorited a post, or yours was favorited | Either direction |
| Boosted a post, or yours was boosted | Either direction |
| Bookmarked or unbookmarked a post | Your own bookmarking |
| Pinned or unpinned a post | Your own pinning |
| Poll ended | A poll you voted in has finished |
| Voted in a poll | You cast a vote |
| Edited a post, or one you follow was edited | Either direction |
| Sent a post or reply | Your post or reply was published or scheduled |
| Deleted a post | You deleted one of your posts |
| Action failed | Any of the above failed |

Blocking, muting, and hiding boosts have no sound, since borrowing the follow sound for
them would be misleading and the screen reader announces them anyway.

Sounds for things that *arrive* (mentions, direct messages, new home posts, and
notifications that somebody favorited, boosted, followed, or edited) follow the
notification mode on the General tab: they play in `Sound only` and `Sound and
notifications`, and are silent in `Classic Windows Notifications` or `Disabled`.

Sounds for things *you do* always play, subject only to this tab, because they confirm
an action you just took rather than announcing something that arrived.

Buttons under the list:

- `Play`: hear the selected event's sound. This works even when the event is unchecked,
  so you can audition a sound before switching it back on.
- `Change...`: pick any `.mp3`, `.wav`, `.ogg`, `.flac`, `.m4a`, `.aac`, or `.opus` file.
  Choosing a file also re-checks that event.
- `Reset`: hand the selected event back to the active pack, dropping any file you chose for it.
- `Reset all`: hand every event back to the active pack.
- `Open sounds folder`: open your personal sound folder, creating it and a `packs` folder
  inside it if they are not there yet, along with a readme listing the filename each event
  answers to.

### Sound Packs

A pack is a folder of sounds named after the events they play for, such as `mention.mp3`
and `follow.mp3`. Packs live under `packs` in your personal sound folder.

Fedra ships no packs. To make one, press `Open sounds folder`, create a folder under
`packs`, name it whatever you like, and put your files in it. It appears in the
`Sound pack` list the next time you open the options. The readme in that folder lists
every filename Fedra looks for.

A pack may use `.mp3`, `.wav`, `.ogg`, `.flac`, or `.m4a`. A pack does not have to be
complete: any event it does not provide falls back to `boop.mp3`, so a pack of one file
is perfectly valid.

Picking a file with `Change...` overrides the pack for that one event and survives
switching packs, until you press `Reset` to hand the event back to the pack.

Updating Fedra never touches your sound folder, so your packs and edits survive an
upgrade.


### Templates Tab
Customize how posts appear in each timeline using [Jinja2-style](https://jinja.palletsprojects.com/en/stable/templates/) templates.

- Select a timeline from the dropdown (or **Global Default** to set the fallback used by all timelines without their own override).
- Edit the **Window title template**, **Post template**, **Boost template**, and **Quote template** text fields.
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
| `{{ app }}` | The application name (window title template only) |
| `{{ timeline }}` | The active timeline name (window title template only) |
| `{{ account }}` | Your `@acct` handle (window title template only) |

#### Conditionals

You can use `{% if %}` blocks to show text only when a variable is non-empty:

```
{% if client %}, via {{ client }}{% endif %}
```

### Filters Tab
Hide post types per timeline, on the client side only. Select a timeline from the dropdown, then check the types you want to hide:
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

These are separate from your instance's own filters, which are managed in `Options -> Manage Filters...`.

## Keyboard Shortcuts

Every shortcut in the table below can be changed in `Options -> Customize Keyboard Shortcuts...`. Normal mode and Quick Action Keys mode have their own independent bindings, and actions listed as `None` have no default binding but can be given one.

### Customizing Shortcuts
The dialog has a **Quick Keys Mode** tab and a **Normal Mode** tab. On each:

- **Enter key behavior** picks between `Enter` opening links with `Alt+Enter` viewing the thread, or the reverse. Binding either action by hand shows this as `Custom`.
- **Set Shortcut...** opens a capture dialog: click in the key field and press the combination you want. The detected chord is announced as you type it, and if the combination is already assigned to another action, you are asked whether to reassign it.
- **Clear Shortcut** unbinds the selected action, **Reset to Default** restores just that action, and **Reset All to Defaults** restores the whole mode.

### Fixed Keys
These are built into the lists and cannot be customized:

- `Tab` / `Shift+Tab`: Move between the Timelines list and the Posts list
- `Up Arrow` / `Down Arrow`: Move by one entry
- `Home` / `End`: Jump to the first or last entry
- `Page Up` / `Page Down`: Move by 20 entries
- `Ctrl+1`..`Ctrl+9`: Switch to timeline 1-9
- `1`..`9`: Switch to timeline 1-9 (Quick Action Keys mode only)
- `Shift+F10` or the applications key: Open the actions menu for the focused post, or for the focused user in the followers/following dialogs

### Global
- `Ctrl+Alt+F`: Show/hide the main window (global hotkey; customizable in Options)

### Default Bindings

| Action | Normal mode | Quick Action Keys mode |
|---|---|---|
| New Post... | `Ctrl+N` | `C` |
| Reply... | `Ctrl+R` | `R` |
| Reply to Author... | `Ctrl+Shift+R` | `Ctrl+R` |
| Quote Post... | `Ctrl+Q` | `Q` |
| Toggle Follow | `Alt+F` | `Alt+F` |
| View Author Profile | `Ctrl+P` | `P` |
| View Mentions | `Ctrl+M` | `M` |
| View Hashtags | `Ctrl+H` | `H` |
| Open Links | `Enter` | `Enter` |
| Play Media | `Ctrl+I` | `I` |
| Open in Browser | `Ctrl+Shift+O` | `O` |
| Copy Post | `Ctrl+Shift+C` | `Ctrl+Shift+C` |
| Copy Post Link | `Ctrl+C` | `Ctrl+C` |
| View Post Details | `Shift+Enter` | `Shift+Enter` |
| View Thread | `Alt+Enter` | `Alt+Enter` |
| View Quoted Thread | None | None |
| Edit Post... | `Ctrl+E` | `E` |
| Delete Post | `Delete` | `Delete` |
| Pin / Unpin Post | None | None |
| Vote on Poll... | `Ctrl+V` | `V` |
| Favorite | `Ctrl+Shift+F` | `F` |
| Bookmark | `Ctrl+Shift+K` | `K` |
| Boost | `Ctrl+Shift+B` | `B` |
| View Boosts | None | None |
| View Favorites | None | None |
| Open User Timeline | `Ctrl+T` | `T` |
| Open User... | `Ctrl+U` | `U` |
| Search... | `Ctrl+/` | `/` |
| Find in Timeline... | `Ctrl+F` | `Ctrl+F` |
| Find Next | `F3` | `F3` |
| Find Previous | `Shift+F3` | `Shift+F3` |
| Home Timeline | None | None |
| Notifications Timeline | None | None |
| Sent Timeline | None | None |
| Local Timeline | `Ctrl+L` | `Ctrl+L` |
| Open Instance Timeline... | `Ctrl+Shift+I` | `Shift+I` |
| Federated Timeline | None | None |
| Direct Messages | `Ctrl+D` | `Ctrl+D` |
| Mentions Timeline | `Ctrl+Shift+M` | `Ctrl+Shift+M` |
| Bookmarks | None | None |
| Favorites | None | None |
| Open List... | None | None |
| Load More | `.` | `.` |
| Close Timeline | `Ctrl+W` | `Backspace` |
| Refresh | `F5` | `F5` |
| Previous Timeline | `Left` | `Left` |
| Next Timeline | `Right` | `Right` |
| Move Timeline Left | `Shift+Left` | `Shift+Left` |
| Move Timeline Right | `Shift+Right` | `Shift+Right` |
| Previous Account | `Ctrl+[` | `Ctrl+[` |
| Next Account | `Ctrl+]` | `Ctrl+]` |
| Toggle Content Warning | `Ctrl+X` | `X` |
| Toggle Quick Keys Mode | `Ctrl+Shift+Q` | `Ctrl+Shift+Q` |
| Manage Accounts... | `Ctrl+Alt+A` | `Ctrl+Alt+A` |
| Manage Filters... | None | None |
| Manage Lists... | None | None |
| Edit Profile... | `Ctrl+Shift+E` | `Ctrl+Shift+E` |
| Options... | `Ctrl+,` | `Ctrl+,` |
| Customize Keyboard Shortcuts... | None | None |
| Check for Updates... | None | None |
| View Help | `F1` | `F1` |

Actions with no default binding are still reachable from the menu bar or the post context menu. **View Boosts** and **View Favorites** only appear in the Post menu when the selected post actually has boosts or favorites, and **Edit Post**, **Delete Post**, and **Pin / Unpin Post** only appear for your own posts.

### Quick Action Keys Mode
Toggle with `Ctrl+Shift+Q`. While it is on, the single-letter bindings in the table above act on the selected post instead of being typed, and `Backspace` closes the current timeline.

## Accounts
`Ctrl+Alt+A` opens the accounts dialog, where you can **Add**, **Remove**, or **Switch To** an account. Adding an account walks you through authorizing Fedra on your instance in the browser. `Ctrl+[` and `Ctrl+]` cycle accounts directly; each account keeps its own set of open timelines, and the newly active account's handle is announced when you switch.

## Profile Editing
`Ctrl+Shift+E` opens your profile for editing:

- Display name and bio
- Avatar and header images
- `Require follow approval`
- `Bot account`
- `Discoverable in directory`
- Default post visibility
- `Mark media as sensitive by default`
- Default post language (ISO code)

## Lists
`Options -> Manage Lists...` shows your Mastodon lists, with buttons to **Add**, **Edit**, view and change **Members**, and **Delete**. Open a list as a timeline with `Timelines -> Open List...`. Individual users can also be added to a list from the **Actions...** menu in the profile and followers/following dialogs. List timelines stream in real time.

## Server-Side Filters
`Options -> Manage Filters...` manages the filters stored on your instance, which apply everywhere you use Mastodon, not just in Fedra. Each filter has a title, the contexts it applies to, an action, an optional expiry, and a list of keywords, each of which can be marked whole-word.

## Finding Text in a Timeline
`Ctrl+F` prompts for text and moves to the next matching entry, respecting your timeline sort direction. `F3` and `Shift+F3` repeat the search forwards and backwards. With `Load more on find next` enabled in the Timeline options, Fedra keeps fetching older posts while searching instead of stopping at the end of what is already loaded.

## Media Player

Press `Ctrl+I` (or `I` in Quick Action Keys mode) on a post with media attachments to open the media player. If the post has multiple attachments, a dialog lets you choose which one to play.

### Media Player Keys

| Key | Action |
|-----|--------|
| `Space` | Play / Pause |
| `Left Arrow` | Seek backward 10 seconds |
| `Right Arrow` | Seek forward 10 seconds |
| `Up Arrow` | Volume up |
| `Down Arrow` | Volume down |
| `E` | Announce elapsed time |
| `R` | Announce remaining time |
| `T` | Announce total duration |
| `D` | Download media file |
| `Escape` | Close media player |

## Search
- Use `Ctrl+/` to open Search.
- Search types:
  - All
  - Accounts
  - Hashtags
  - Posts
- Results open in a dedicated timeline (`Search: <query>`) and support paging.
- `Alt+Enter` on an account or hashtag result opens its timeline.

## Links in Posts
`Enter` on a post opens its links. If the post has more than one link, or `Always prompt to open links` is enabled, a dialog lists them with **Open** and **Copy** buttons. Tracking parameters are stripped from URLs unless you turn that off in the General options.

## Configuration File
- Installed build: `%APPDATA%\Fedra\config.json`
- Portable/uninstalled run: `config.json` next to the executable

Sound settings live under the `sounds` key: `enabled` is the master switch, `volume` is 0 to 100,
`pack` names the active pack, and `events` maps an event name to its `enabled` flag and `file`. An
empty `file` means the event takes its sound from the pack, or from `boop.mp3` if the pack
does not provide one; a bare name is looked up in your `sounds` folder and then in Fedra's,
and an absolute path is used as given.

## Changelog

### Unreleased
* Added per-event notification sounds. Mentions, direct messages, new home-timeline posts, new
  followers, follow requests, favorites, boosts, bookmarks, pins, ended polls, poll votes, edited
  posts, sent posts, deleted posts, and failures each have their own sound instead of sharing a
  single beep.
* Sounds are tied to actions in both directions, so favoriting a post and having one of yours
  favorited play the same sound, as do boosting, following, and editing.
* Added a Sounds tab to the options dialog for setting the master switch, the volume, and the sound
  used by each event, with a Play button to audition one and a file picker to replace it.
* Added a `Sound and notifications` notification mode, for a Windows notification and a sound
  together.
* Added sound packs, so a whole set of sounds can be swapped in one step rather than fifteen
  files at a time. A pack is a folder of files named after the events they play for.
* No per-event audio is bundled. Every event falls back to the existing `boop.mp3`, so an
  untouched install sounds exactly as it did before, and the sounds you do hear are ones you
  chose.
* Custom sounds and packs are read from `sounds` in your configuration folder, which takes
  precedence over the sounds Fedra ships with and is left alone by updates.

### Version 0.5.0
* Account usernames are now properly resolved when replying to a post from a remote instance's local timeline.
* Added a hotkey customization dialog! It is now possible to change any keybinding in Fedra, for both the regular and quick key modes, through a simple and intuitive dialog.
* Added a sent timeline option, to bring up your current account's timeline in a buffer.
* Added a view timeline button to the hashtags dialog.
* Added an add to list option to the actions menu for a user.
* Added Ctrl+C as a shortcut to copy the focused post's link.
* Added page up and page down keyboard shortcuts to the timeline list, allowing you to move by 20 posts at a time.
* Added shortcut keys to controls in the compose post dialog.
* Added shortcuts to the media player dialog to speak your loaded track's elapsed, remaining, and total times, bound to e, r, and t respectively.
* Added the ability to customize your window title using a template.
* Fixed copying posts in user timelines.
* Fixed editing a post with the content-type set to markdown, you'll now be able to edit your original markdown content rather than the Mastodon-rendered text.
* Fixed link previews getting coppied when copying post text.
* Fixed opening user timelines from the local timeline of a remote instance.
* Fixed posts sometimes automatically rereading in the timeline list.
* Indentation in post bodies is now properly preserved.
* It is now possible to view the quoter's profile or timeline when on a quoted post, Similar to how boosted posts already work.
* Pressing the applications key in the followers or following dialogs will now bring up the actions menu.
* Removed the limitations on what timelines you must have open at all times. In other words, it is now possible to close your home and notification timelines if you so desire. Additionally added home and notification options to the timeline menu to bring them back.
* Timelines without proper web socket streaming support should now correctly refresh periodically.
* Trailing dashes and other junk are now stripped from the end of post bodies when copying them.

### Version 0.4.0
* Added an actions button to the follower/following dialogs, working the exact same way as it does in the view profile dialog.
* Find in timeline now respects your timeline sort direction.
* Fixed a bug where going to the bottom of a thread, hitting home, and then performing an action would perform that action on the post you were previously on, not the newly focused one.
* Fixed Fedra crashing when exiting from the system tray.
* Fixed hashtags showing in the mentions dialog as @tags@instance.domain.
* Fixed modal dialogs not stacking how you'd expect, leading to you sometimes ending up with a bunch of ghost dialogs that you'd only discover when hiding Fedra's window.
* Fixed quote posts not rendering properly in the webview.
* Fixed streaming not working on instances such as mastodon.social.
* Fixed the compose dialog closing and taking your post content with it on error.
* Fixed your list position being randomly moved up a few items sometimes.
* Follower relationships are now shown in the follower/following dialogs.
* It is now possible to mark media as sensitive.
* It is now possible to mute/unmute hashtags directly in Fedra.
* Made Fedra expand quote posts much more reliably.
* Opening a thread will now put you on the post you selected from that thread, not the first post.
* Sensitive media in posts is now properly handled by Fedra.
* Swapped the open link and view thread hotkeys, so now enter opens links in posts and alt+enter opens the thread.
* Switched to a fully custom list control, backed by [AccessKit](https://accesskit.dev), to prevent screen readers from rereading the focused item every minute among other things.
* The followers/following dialogs now properly fetch users from remote instances, and give you progress as they load the lists.
* The media player dialog will now be properly focused after downloading media.
* Various little UI tweaks, for example adding accelerators where there previously were none.

### Version 0.3.1
* Added a mentions timeline.
* Added an option to open the local timeline for a specific instence.
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
