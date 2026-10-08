# Fedra User Manual

[Fedra](https://github.com/trypsynth/fedra) is a native, keyboard-first Mastodon client for Windows and macOS, made for screen reader users.

## System Requirements
- Windows 10 or 11, on x64 or Arm.
- macOS 13 Ventura or later, on Apple silicon.

## Core Features
- A native user interface with controls that work well with screen readers. Fedra announces changes as they occur. On Windows, the timeline is a custom list control that uses [AccessKit](https://accesskit.dev).
- More than one account. Each account keeps its own open timelines when you switch between accounts.
- Timelines: Home, Notifications, Mentions, Sent, Local, the Local timeline of another instance, Federated, Direct Messages, Bookmarks, Favorites, Lists, User, Hashtag, Thread, and Search.
- Live updates for the Home, Notifications, Local, Federated, Direct Messages, and list timelines. Your own posts also show in the Sent timeline when you publish them.
- Post creation and editing, with:
  - Visibility: Public, Unlisted, Followers only, or Direct.
  - Content warnings.
  - Content type: Default, plain text, Markdown, or HTML.
  - Post language.
  - Media attachments with descriptions. You can mark media as sensitive.
  - Polls, with preset durations, multiple choices, and hidden vote counts.
  - Quote posts.
  - Scheduled posts.
  - Thread mode, to write a series of replies to your own posts in one dialog.
- Tools for people and topics:
  - Open a profile or timeline from a post, a mention, a list of boosts or favorites, or a search result.
  - Follow, unfollow, block, and mute users. Show or hide the boosts of a user. Add users to lists.
  - Accept or reject follow requests.
  - See the followers and the accounts that a user follows.
  - Give a user an alias. Fedra shows the alias in place of their display name.
  - Follow, unfollow, and mute hashtags.
  - See the users who boosted or favorited a post.
  - Search for accounts, hashtags, and posts.
- A media player that can also download media.
- Keyboard shortcuts that you can change. Normal mode and Quick Action Keys mode each have their own shortcuts.
- Timeline filters in Fedra, and management of the filters on your server.
- Management of Mastodon lists, and list timelines.
- Templates for timeline entries and for the window title.
- A tray icon. Global shortcuts let you read and use your timelines from any program, also when the main window is hidden.
- Update checks at startup or when you ask, for stable releases or test builds.

## Main Window Layout
The main window has two lists:

- The Timelines list shows all open timelines, in order.
- The Posts list shows the entries of the selected timeline.

Use `Tab` and `Shift+Tab` to move between the two lists. You can switch, move, and close timelines from either list. Use the Posts list for actions on posts.

## Timelines

### Opening Timelines
Open a timeline from the Timelines menu, from a post, or with its shortcut. You can open user timelines, threads, and hashtag timelines from a post. All open timelines show in the Timelines list. A timeline stays open until you close it with `Ctrl+W`, or `Backspace` in Quick Action Keys mode.

| Timeline | How to open |
|---|---|
| Home | `Timelines -> Home Timeline` |
| Notifications | `Timelines -> Notifications` |
| Mentions | `Ctrl+Shift+M` |
| Sent | `Timelines -> Sent` |
| Local | `Ctrl+L` |
| Local timeline of another instance | `Ctrl+Shift+I`, then type the domain |
| Federated | `Timelines -> Federated Timeline` |
| Direct Messages | `Ctrl+D` |
| Bookmarks | `Timelines -> Bookmarks` |
| Favorites | `Timelines -> Favorites` |
| List | `Timelines -> Open List...` |
| User | `Ctrl+T` on a post, or `Ctrl+U` to type a handle |
| Hashtag | `Ctrl+H` on a post, then `View Timeline` |
| Thread | `Alt+Enter` on a post |
| Search | `Ctrl+/` |

Fedra opens Home and Notifications at startup. You can close them as you close other timelines, and open them again from the Timelines menu.

### The Sent Timeline
`Timelines -> Sent` opens the timeline of your own account. It shows all your posts, replies, and boosts, with your pinned posts at the top. When you publish or delete a post, the Sent timeline changes immediately.

### Moving and Switching
To move a timeline, press `Shift+Left Arrow` or `Shift+Right Arrow` in either list. To switch timelines, press `Left Arrow` or `Right Arrow`, or press `Ctrl+1` through `Ctrl+9`.

### Permanent Timelines
Make Timeline Permanent or Closable (`Ctrl+Shift+P`, or `Ctrl+P` in Quick Action Keys mode) makes the current timeline permanent. You can't close a permanent timeline until you press the shortcut again. Fedra saves permanent timelines for each account.

### New Post Notifications
Turn New Post Notifications On or Off (`Ctrl+Shift+N`, or `Ctrl+N` in Quick Action Keys mode) tells you when new posts arrive in the current timeline. Fedra uses the notification mode that you set in the options: a system notification, a sound, or nothing. This works in the timelines that update live: Home, Local, Federated, Direct Messages, and lists. Many posts that arrive together give one notification. Your own posts never give a notification. Fedra saves this setting for each account.

To know which timeline a notification is from, give each timeline its own sound. Use `Timelines -> Set Notification Sound...` to select a sound file for the current timeline. Fedra plays MP3, Ogg, WAV, FLAC, and M4A files. To use the usual sound again, use `Timelines -> Use the Default Notification Sound`. Timeline sounds play when the notification mode is Sound only. A sound that you set on the Notifications timeline also plays for usual notifications.

### Clearing
Clear Timeline (`Ctrl+Delete`) removes all entries from the current timeline. Clear All Timelines (`Ctrl+Shift+Delete`) removes all entries from all open timelines. This changes only what Fedra shows. Nothing is deleted from your server, and new posts continue to arrive. To show the latest posts again, refresh the timeline.

### Refreshing
Timelines that update live refresh automatically. Press `F5` to refresh the current timeline. Press `.` (Load More) to get older entries. If a live timeline loses its connection, Fedra gets it again about once a minute until the connection comes back.

## Window Visibility and Tray
- The tray icon has a menu with two items:
  - `Show/Hide`
  - `Exit`
- A global shortcut shows and hides the main window. The default is `Ctrl+Alt+F`. To change it, use the Global tab of `Options -> Customize Keyboard Shortcuts...`.
- Other global shortcuts let you use Fedra when its window is hidden. See [Global Shortcuts](#global-shortcuts).

## Composing Posts
`Ctrl+N` opens the compose dialog. These shortcuts also open it:

- `Ctrl+R` for a reply.
- `Ctrl+Shift+R` for a reply to the author only.
- `Ctrl+Q` for a quote.
- `Ctrl+E` to edit your post.

All controls have an access key. The dialog title shows the number of characters that you can still type on your instance. You can type more than the limit, but Fedra plays a warning sound when you do.

The dialog has these controls:

- Content warning: a check box and a field for the warning text.
- Content type: Default, plain text, Markdown, or HTML, on instances that support it. When you edit a Markdown post, you get your original Markdown, not the rendered text.
- Visibility: Public, Unlisted, Followers only, or Direct. It starts with the default visibility of your account.
- Post language: a code such as `en` or `de`. It starts with the setting of your account.
- `Manage Media...`: add attachments, add a description to each one, and mark them as sensitive.
- `Add Poll...`: add options up to the limit of your instance. Select a duration, allow more than one choice, and hide the vote counts until the poll ends.
- `Schedule...`: select a local date and time to publish the post. To post immediately, select `Clear Schedule`.
- Thread mode: when you select it, the dialog opens again after you post, as a reply to your new post. This lets you write a thread in one dialog.

If `Use enter to send posts` is on, press `Enter` in the content field to post. If it is off, use the Post button.

## Options
To open the options, press `Ctrl+,`.

### General Tab
- `Use enter to send posts`
- `Always prompt to open links`
- `Read link previews in timelines`
- `Strip tracking parameters from URLs`
- `Use quick action keys in timelines`
- `Check for updates on startup`
- Updates:
  - `Stable releases`
  - `Test builds`
- Notification mode:
  - Operating system notifications
  - Sound only
  - Disabled
- `Notification Types...`: select the notifications that you get. The dialog has two groups. `Show in timelines` sets which types show in the Notifications timeline. `Alert me with a sound or notification` sets which of these types also play a sound or show a notification. For example, you can keep favorites and boosts in the timeline to read later, without interruptions when they arrive. All types are on by default. The types are:
  - Mentions
  - Boosts
  - Favorites
  - New followers
  - Follow requests
  - Poll results
  - Edited posts
  - New posts
  - Moderation and admin
- `Customize Keyboard Shortcuts...`

### Invisible Interface Tab
This tab shows only on Windows.

- `Use the invisible interface`: turns the [global shortcuts](#global-shortcuts) on or off. The shortcut that shows and hides the window always works, so you can always get a hidden window back.
- `Repeat the post at the start or end of a timeline`: when this is on and you move past the first or last post with a global shortcut, Fedra reads that post again. When it is off, Fedra says nothing.

### Timeline Tab
- `Restore open timelines on startup`: when this is off, Fedra opens only your default timelines at startup.
- `Sync home and notifications position with your server`: Fedra saves your position in Home and Notifications on your server about once a minute, and when you exit. At startup, Fedra goes to that position. Other apps that sync their position, such as Tusky or Ivory, use the same position.
- `Load older posts to find your saved position`: if your saved position is older than the first page of posts, Fedra loads older posts until it finds it. Fedra loads up to 10 pages. If the post was deleted, Fedra goes to the next older post.
- Autoload posts:
  - Never
  - When reaching the end
  - When navigating past the end
- Posts to load at a time (`1` to `40`)
- `Stream new posts in real time`: when this is off, Fedra checks for new posts on a timer instead.
- Minutes between checks for new posts when not streaming (`1` to `60`): also used for any timeline whose stream is down.
- Content warning display:
  - Show inline
  - Don't show
  - CW only
- Hide emoji in names:
  - None
  - Unicode emojis
  - Instance emojis
  - All
- `Show oldest timeline entries first`
- `Always preserve thread order`
- `Load more on find next`
- `Customize Default Timelines...`:
  - Fedra always opens Home and Notifications at startup.
  - You can add Local, Federated, Direct Messages, Bookmarks, Favorites, Mentions, and Sent.

### Templates Tab
Templates set how posts show in each timeline. They use [Jinja2](https://jinja.palletsprojects.com/en/stable/templates/) syntax.

- Select a timeline in the list. To set the templates for all timelines that don't have their own, select `Global Default`.
- Edit the `Window title template`, `Post template`, `Boost template`, and `Quote template` fields.
- When you select the Notifications timeline, a `Favorite template` field also shows. It sets how "X favorited your post" entries show, as the boost template does for boosts. Other timelines don't show favorite notifications, so the field doesn't show for them.
- To set the templates of the selected timeline back to the global default, select `Reset to default`. For the global default, this sets the built-in templates again.

Fedra renders the template for each entry each time it shows a timeline. If a template has a syntax error, the entry shows as `author: content`.

#### Available Variables

| Variable | Value |
|---|---|
| `{{ author }}` | Display name. Uses the Hide emoji in names setting. |
| `{{ username }}` | `@acct` handle |
| `{{ content }}` | Post text without HTML. Uses the Content warning display setting. |
| `{{ content_warning }}` | Content warning text, or empty |
| `{{ relative_time }}` | Relative time, for example `2 hours ago` |
| `{{ absolute_time }}` | Local date and time, for example `Feb 17, 2026 at 2:30 PM` |
| `{{ visibility }}` | `Public`, `Unlisted`, `Followers only`, or `Direct` |
| `{{ reply_count }}` | For example, `3 replies` |
| `{{ boost_count }}` | For example, `1 boost` |
| `{{ favorite_count }}` | For example, `5 favorites` |
| `{{ client }}` | Name of the app that posted, or empty if not known |
| `{{ media }}` | Summary of media attachments, or empty |
| `{{ poll }}` | Summary of the poll, or empty |
| `{{ booster }}` | Display name of the user who boosted. Boost template only. Empty for usual posts. |
| `{{ booster_username }}` | `@acct` handle of the user who boosted. Boost template only. |
| `{{ favoriter }}` | Display name of the user who favorited your post. Favorite template only. |
| `{{ favoriter_username }}` | `@acct` handle of the user who favorited your post. Favorite template only. |
| `{{ quote_author }}` | Display name of the author of the quoted post. Quote and boost templates. |
| `{{ quote_username }}` | `@acct` handle of the author of the quoted post. Quote and boost templates. |
| `{{ quote_content }}` | Text of the quoted post. Quote and boost templates. |
| `{{ quote_media }}` | Media summary of the quoted post. Quote and boost templates. |
| `{{ quote_poll }}` | Poll summary of the quoted post. Quote and boost templates. |
| `{{ app }}` | Name of the app. Window title template only. |
| `{{ timeline }}` | Name of the current timeline. Window title template only. |
| `{{ account }}` | Your `@acct` handle. Window title template only. |

#### Conditionals

To show text only when a variable is not empty, use an `{% if %}` block:

```
{% if client %}, via {{ client }}{% endif %}
```

### Filters Tab
Hide types of posts in each timeline. These filters apply only in Fedra. Select a timeline in the list, then select the types to hide:

- Original posts (not replies or boosts)
- Replies to others
- Replies to me
- Threads (replies to your own posts)
- Boosts
- Quote posts
- Posts with media
- Posts without media
- Your posts
- Your replies

The filters on your server are different. To manage them, use `Options -> Manage Filters...`.

## Keyboard Shortcuts

You can change all the shortcuts in the tables below in `Options -> Customize Keyboard Shortcuts...`. Normal mode and Quick Action Keys mode each have their own shortcuts. An action that shows `None` has no default shortcut, but you can give it one.

### Customizing Shortcuts
The dialog has a Quick Keys Mode tab and a Normal Mode tab. On Windows, it also has a Global tab. Each tab has these buttons:

- `Set Shortcut...`: opens a dialog. Select the key field, then press the key combination. Fedra announces the combination as you press it. If another action already uses the combination, Fedra asks if you want to assign it to this action.
- `Clear Shortcut`: removes the shortcut from the selected action.
- `Reset to Default`: sets the default shortcut for the selected action.
- `Reset All to Defaults`: sets the default shortcuts for all actions on the tab.

### Fixed Keys
You can't change these keys:

- `Tab` and `Shift+Tab`: move between the Timelines list and the Posts list.
- `Up Arrow` and `Down Arrow`: move one entry.
- `Home` and `End`: go to the first or last entry.
- `Page Up` and `Page Down`: move 20 entries.
- `Ctrl+1` through `Ctrl+9`: switch to timeline 1 through 9.
- `1` through `9`: switch to timeline 1 through 9. Quick Action Keys mode only.
- `Shift+F10` or the Applications key: open the actions menu for the selected post, or for the selected user in the followers and following dialogs.

### Global Shortcuts
Global shortcuts work from all programs, when the Fedra window is visible, hidden, or behind other windows. When you move through posts, Fedra reads each post through your screen reader, if one is running. When you switch timelines, Fedra reads the name of the timeline and the post that you go to. You can also use almost all the actions of Normal mode and Quick Action Keys mode as global shortcuts.

Global shortcuts are off by default. To turn them on, select `Use the invisible interface` on the Invisible Interface tab of the options. The shortcut that shows and hides the window always works. To change, clear, or add global shortcuts, use the Global tab of `Options -> Customize Keyboard Shortcuts...`.

Each default global shortcut is the Normal mode shortcut for the same action, with `Ctrl`, `Alt`, and `Win` added. For example, `Ctrl+P` for a profile becomes `Ctrl+Alt+Win+P`. The arrow keys that move through the list become `Ctrl+Alt+Win` with an arrow key. The show and hide shortcut stays `Ctrl+Alt+F`.

Some actions have no default global shortcut, because their keys conflict with other actions or with Windows. These include Find, View Thread, Delete Post, Open in Browser, Clear Timeline, and Exit. `Ctrl+Alt+Shift+Win` is the Office key. Thus, no default uses it with a letter that opens an Office app, such as O for Outlook.

| Action | Default |
|---|---|
| Show or hide window | `Ctrl+Alt+F` |
| Previous post | `Ctrl+Alt+Win+Up` |
| Next post | `Ctrl+Alt+Win+Down` |
| First post | `Ctrl+Alt+Win+Home` |
| Last post | `Ctrl+Alt+Win+End` |
| Read current post | `Ctrl+Alt+Win+Space` |
| Previous timeline | `Ctrl+Alt+Win+Left` |
| Next timeline | `Ctrl+Alt+Win+Right` |
| New Post... | `Ctrl+Alt+Win+N` |
| Reply... | `Ctrl+Alt+Win+R` |
| Reply to Author... | `Ctrl+Alt+Shift+Win+R` |
| Quote Post... | `Ctrl+Alt+Win+Q` |
| Toggle Follow | `Ctrl+Alt+Win+F` |
| View Author Profile | `Ctrl+Alt+Win+P` |
| View Mentions | `Ctrl+Alt+Win+M` |
| View Hashtags | `Ctrl+Alt+Win+H` |
| Open Links | `Ctrl+Alt+Win+Enter` |
| Play Media | `Ctrl+Alt+Win+I` |
| Copy Post | `Ctrl+Alt+Shift+Win+C` |
| Copy Post Link | `Ctrl+Alt+Win+C` |
| View Post Details | `Ctrl+Alt+Shift+Win+Enter` |
| Edit Post... | `Ctrl+Alt+Win+E` |
| Vote on Poll... | `Ctrl+Alt+Win+V` |
| Favorite | `Ctrl+Alt+Shift+Win+F` |
| Bookmark | `Ctrl+Alt+Shift+Win+K` |
| Boost | `Ctrl+Alt+Shift+Win+B` |
| Open User Timeline | `Ctrl+Alt+Win+T` |
| Open User... | `Ctrl+Alt+Win+U` |
| Search... | `Ctrl+Alt+Win+/` |
| Find Next | `Ctrl+Alt+Win+F3` |
| Find Previous | `Ctrl+Alt+Shift+Win+F3` |
| Local Timeline | `Ctrl+Alt+Win+L` |
| Open Instance Timeline... | `Ctrl+Alt+Shift+Win+I` |
| Direct Messages | `Ctrl+Alt+Win+D` |
| Mentions Timeline | `Ctrl+Alt+Shift+Win+M` |
| Load More | `Ctrl+Alt+Win+.` |
| Close Timeline | `Ctrl+Alt+Win+W` |
| Refresh | `Ctrl+Alt+Win+F5` |
| Move Timeline Left | `Ctrl+Alt+Shift+Win+Left` |
| Move Timeline Right | `Ctrl+Alt+Shift+Win+Right` |
| Previous Account | `Ctrl+Alt+Win+[` |
| Next Account | `Ctrl+Alt+Win+]` |
| Toggle Content Warning | `Ctrl+Alt+Win+X` |
| Manage Accounts... | `Ctrl+Alt+Win+A` |
| Edit Profile... | `Ctrl+Alt+Shift+Win+E` |
| Options... | `Ctrl+Alt+Win+,` |
| View Help | `Ctrl+Alt+Win+F1` |

If another program already uses one of these shortcuts, Fedra tells you at startup. That shortcut doesn't work until you change it.

### Default Shortcuts

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
| Make Timeline Permanent or Closable | `Ctrl+Shift+P` | `Ctrl+P` |
| Turn New Post Notifications On or Off | `Ctrl+Shift+N` | `Ctrl+N` |
| Clear Timeline | `Ctrl+Delete` | `Ctrl+Delete` |
| Clear All Timelines | `Ctrl+Shift+Delete` | `Ctrl+Shift+Delete` |
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
| Message Requests... | None | None |
| Edit Profile... | `Ctrl+Shift+E` | `Ctrl+Shift+E` |
| Options... | `Ctrl+,` | `Ctrl+,` |
| Customize Keyboard Shortcuts... | None | None |
| Check for Updates... | None | None |
| View Help | `F1` | `F1` |

You can use actions that have no default shortcut from the menu bar or from the context menu of a post. View Boosts and View Favorites show in the Post menu only when the selected post has boosts or favorites. Edit Post, Delete Post, and Pin / Unpin Post show only for your own posts.

### Quick Action Keys Mode
Press `Ctrl+Shift+Q` to turn this mode on or off. When it is on, the single-letter shortcuts in the table above do actions on the selected post. They don't type letters. `Backspace` closes the current timeline.

## Message Requests
Mastodon 4.3 and later keep private mentions from people that you don't follow as message requests. These don't show in your notifications. Fedra checks for message requests about once a minute. When new ones arrive, Fedra tells you in your notification mode. The Options menu shows how many requests wait, for example `Message Requests (2)...`.

Open `Options -> Message Requests...` to see who sent each request, how many messages they sent, and the latest message. Accept or dismiss each request. When you accept a request, its messages show in your notifications and direct messages.

The same thing can occur to your own direct messages. Mastodon doesn't tell the sender. Thus, when a user doesn't follow you, the relationship section of their profile tells you that your direct messages to them can arrive as a message request.

## User Aliases
To show a user under a name that you select:

1. Open their profile, or find them in a followers or following list.
2. Open the actions menu.
3. Select `Set Alias...`.

The alias shows in place of their display name in all your timelines, for all your accounts in Fedra. To show their display name again, set the alias again and leave it empty.

## Accounts
`Ctrl+Alt+A` opens the accounts dialog. Use the `Add...`, `Remove`, and `Switch To` buttons to manage your accounts. When you add an account, Fedra opens your instance in the browser, where you authorize Fedra. `Ctrl+[` and `Ctrl+]` switch to the previous or next account. Each account keeps its own open timelines. When you switch, Fedra announces the handle of the new account.

## Profile Editing
`Ctrl+Shift+E` opens your profile for editing. You can change:

- The display name and bio.
- The avatar and header images.
- The profile fields, up to the limit of your instance.
- `Require follow approval`
- `Bot account`
- `Discoverable in directory`
- The default post visibility.
- `Mark media as sensitive by default`
- The default post language, as a code such as `en` or `de`.

## Lists
`Options -> Manage Lists...` shows your Mastodon lists. Use its buttons to add, edit, and delete lists, and to see and change their members. To open a list as a timeline, use `Timelines -> Open List...`. You can also add a user to a list from the `Actions...` menu in the profile, followers, and following dialogs. List timelines update live.

## Server-Side Filters
`Options -> Manage Filters...` manages the filters on your instance. These filters apply in all Mastodon apps, not only in Fedra. Each filter has a title, the contexts where it applies, an action, an optional expiry time, and a list of keywords. You can set each keyword to match only whole words.

## Finding Text in a Timeline
`Ctrl+F` asks for text, then goes to the next entry that contains it. The search follows the sort order of your timeline. `F3` finds the next match, and `Shift+F3` finds the previous match. If `Load more on find next` is on in the Timeline options, Fedra loads older posts during the search. If it is off, the search stops at the end of the loaded posts.

## Media Player

Press `Ctrl+I`, or `I` in Quick Action Keys mode, on a post with media to open the media player. If the post has more than one attachment, select the attachment to play in the dialog that opens.

### Media Player Keys

| Key | Action |
|---|---|
| `Space` | Play or pause |
| `Left Arrow` | Go back 10 seconds |
| `Right Arrow` | Go forward 10 seconds |
| `Home` | Go to the start |
| `End` | Go to the end |
| `Up Arrow` | Increase the volume |
| `Down Arrow` | Decrease the volume |
| `E` | Announce the elapsed time |
| `R` | Announce the remaining time |
| `T` | Announce the total time |
| `D` | Download the media file |
| `Escape` | Close the media player |

## Search
- Press `Ctrl+/` to open the search.
- Search types:
  - All
  - Accounts
  - Hashtags
  - Posts
- The results open in a new timeline, named `Search: <query>`. You can load more results.
- To open the timeline of an account or hashtag in the results, press `Alt+Enter` on it.

## Links in Posts
Press `Enter` on a post to open its links. If the post has more than one link, or if `Always prompt to open links` is on, a dialog shows the links. Use its `Open` and `Copy` buttons. Fedra removes tracking parameters from URLs, unless you turn this off in the General options.

## Configuration File
- Windows, installed: `%APPDATA%\Fedra\config.json`
- Windows, portable: `config.json` in the same folder as the executable
- macOS: `~/Library/Application Support/Fedra/config.json`

## Changelog

### Version 0.7.0
- Added Clear Timeline and Clear All Timelines.
- Added a macOS version: a native app on a drag-to-install disk image, signed and notarized by Apple.
- Added a note to the profiles of people who don't follow you that your direct messages may reach them as a message request.
- Added a run at startup option to the installer, which keeps Fedra hidden at startup if you left it hidden.
- Added an invisible interface: global shortcuts that move through your timelines, read posts, and perform almost any Fedra action from anywhere, spoken through your screen reader. Turn it on from the new Invisible Interface tab of the options dialog.
- Added an option to sync your Home and Notifications position with your server, and one to load older posts until Fedra finds your saved position.
- Added message requests. When your server holds back private mentions from people you don't follow, Fedra tells you, and `Options -> Message Requests...` lets you accept or dismiss them.
- Added new post notifications for any timeline that updates live, and an optional notification sound of its own for each timeline.
- Added permanent timelines, which can't be closed until you make them closable again.
- Added the alt text of media attachments to the post details dialog.
- Added user aliases, to show anyone under a name you choose.
- Autocomplete now works inline as you type.
- Classic Windows Notifications is now called operating system notifications.
- Fedra is about a fifth smaller.
- Notification types can now show in timelines without making a sound or notification.
- Open timelines are now saved per account, and whenever they change.
- Profile editing now shows as many fields as your instance allows.
- Fixed long profile field values widening the edit profile dialog past the window.
- Fixed numpad keys triggering the wrong shortcuts with Num Lock off.
- Fixed pressing Space while media was loading needing another press to start playback.
- Fixed replying to your own post mentioning yourself.
- Fixed the relationship heading showing on profiles with nothing under it.

### Version 0.6.0
- Added a customizable favorite template, with the `{{ favoriter }}` and `{{ favoriter_username }}` variables.
- Added a notification types dialog to the General options. Select the types of notifications that you get.
- Added a Send Direct Message action to the user actions menu.
- Added an action to the user actions menu to turn notifications on or off for a user that you follow.
- Added an image viewer. Image attachments no longer open in the audio player.
- Added View Boosts and View Favorites to the post context menu.
- Attachments without a description are no longer announced as "alt N: (missing)".
- Fixed Copy Post being unavailable on a thread with only one post.
- Fixed Play Media reporting no media on a quote post when the media is on the quoted post.
- Fixed posts disappearing from live timelines when an instance sent a field that Fedra didn't expect.
- Fixed quoted posts reading "RE:" and a link when the quoted post was also a quote.
- Fixed refreshing a timeline removing the posts that you loaded with Load More.
- Fixed the updater failing with an asset error.
- Fixed the nested quote in a quoted thread showing incorrectly until you refreshed.
- Fixed timelines not refreshing on instances without working live updates, such as GoToSocial.
- Media playback no longer needs the Windows Media Player Legacy optional feature.
- Removed the Enter key behavior control from the shortcuts dialog.
- Screen readers no longer read the current post again when the timeline refreshes in the background.
- The Mentions and Sent timelines now have their own templates and filters.
- The posts to fetch setting now applies to all fetches, not only to Load More.

### Version 0.5.1
- Added native Arm64 builds for Windows on Arm devices.
- `Ctrl+Enter` now sends posts from all controls in the compose dialog.
- Fixed Fedra sometimes doing an action on the wrong post in timelines.
- Fixed pressing `Ctrl` alone sometimes doing the Delete Post action.
- If the media player can't start, it now tells you how to install the Windows Media Player Legacy optional feature, instead of showing a general error.

### Version 0.5.0
- Fedra now finds the correct account names when you reply to a post from the Local timeline of another instance.
- Added a dialog to change the keyboard shortcuts. You can change all shortcuts in Fedra, for Normal mode and Quick Action Keys mode.
- Added the Sent timeline, which shows the timeline of your current account.
- Added a View Timeline button to the hashtags dialog.
- Added an Add to List option to the actions menu for a user.
- Added `Ctrl+C` to copy the link of the selected post.
- Added `Page Up` and `Page Down` to the timeline list, to move 20 posts at a time.
- Added access keys to the controls in the compose dialog.
- Added shortcuts to the media player to announce the elapsed, remaining, and total time: `E`, `R`, and `T`.
- Added a template for the window title.
- Fixed copying posts in user timelines.
- Fixed editing a Markdown post. You now edit your original Markdown, not the text that Mastodon rendered.
- Fixed link previews being copied when you copy the text of a post.
- Fixed opening user timelines from the Local timeline of another instance.
- Fixed posts sometimes being read again in the timeline list.
- Fedra now keeps the indentation in posts.
- You can now open the profile or timeline of the user who quoted a post, as you can for boosted posts.
- The Applications key now opens the actions menu in the followers and following dialogs.
- Removed the timelines that you always had to keep open. You can now close the Home and Notifications timelines. The Timelines menu has items to open them again.
- Timelines without working live updates now refresh at regular intervals.
- Fedra now removes trailing dashes and other extra characters from the end of posts when you copy them.

### Version 0.4.0
- Added an actions button to the followers and following dialogs. It works as it does in the profile dialog.
- Find in Timeline now follows the sort order of your timeline.
- Fixed actions applying to the previous post after you went to the bottom of a thread and pressed `Home`.
- Fixed Fedra stopping when you exited from the tray.
- Fixed hashtags showing as @tags@instance.domain in the mentions dialog.
- Fixed dialogs showing in the wrong order. Sometimes, extra dialogs showed only when you hid the Fedra window.
- Fixed quote posts showing incorrectly in the post details dialog.
- Fixed live updates on instances such as mastodon.social.
- Fixed the compose dialog closing and losing your post when an error occurred.
- Fixed your position in the list sometimes moving up a few items.
- The followers and following dialogs now show your relationship with each user.
- You can now mark media as sensitive.
- You can now mute and unmute hashtags in Fedra.
- Fedra now opens quote posts much more reliably.
- When you open a thread, Fedra now goes to the post that you selected, not the first post.
- Fedra now handles sensitive media in posts correctly.
- Swapped the Open Links and View Thread shortcuts. `Enter` now opens links, and `Alt+Enter` opens the thread.
- Changed to a custom list control that uses [AccessKit](https://accesskit.dev). This stops screen readers from reading the selected item again every minute, and fixes other problems.
- The followers and following dialogs now load users from other instances correctly, and show progress while they load.
- The media player now gets the focus after a download.
- Added access keys and made other small changes to the user interface.

### Version 0.3.1
- Added a Mentions timeline.
- Added an option to open the Local timeline of a specific instance.
- Added media playback and download for posts.
- Fixed Quick Action Keys mode not turning off until you moved in the list.
- The select user dialog no longer shows the same user two times when a user boosts their own post.
- Fedra now goes back to your last post at startup, if it can get that post.

### Version 0.3.0
- Added an option to hide the totals of polls. Poll durations now use preset times.
- Added an option to open your previous timelines again at startup.
- Added an option to show link previews in the timeline.
- Added more file types to the add media dialog.
- Added management of lists, and list timelines.
- Added support for reading and writing quote posts.
- Added a Filters tab to the options, to filter your timelines in Fedra.
- Added scheduled posts.
- Added the `{{ booster_username }}` template variable, to show handles the same way everywhere.
- Added a thread mode check box to the compose dialog. When it is on, the dialog opens again after each post, as a reply to your previous post.
- The compose dialog now uses the default post visibility of your account.
- Fixed duplicate messages in the Direct Messages timeline.
- Fixed the description fields not showing in the add media dialog.
- Fixed the post context menu not showing shortcuts and post actions such as Edit and Delete.
- Fixed the post context menu showing incorrect labels on boosted and favorited posts.
- You can now accept and reject follow requests.
- You can now move timelines with `Ctrl+Shift+Left Arrow` and `Ctrl+Shift+Right Arrow`.
- You can now search your timelines with `Ctrl+F`, `F3`, and `Shift+F3`.
- List timelines now update live.
- The select user dialog now opens much faster.
- User timelines now show pinned posts at the top.
- Removed the global template system, which had problems. We plan to write a more stable version.
- The default templates now hide the reply, boost, and favorite counts when they are zero.
- The post details dialog now opens much faster.
- The timeline switching shortcuts now also work in the Timelines list.
- You can now pin and unpin posts.

### Version 0.2.0
- Added a dialog that shows the full contents of a post.
- Added an option to remove tracking parameters from URLs. It is on by default.
- Added an option to check for test builds instead of stable releases.
- Added timeline templates, to set how Fedra shows timeline entries. The relative and absolute time check box is removed from the options. You can now set it in each template. For more information, see the Templates Tab section.
- Fedra now uses your filters in the timeline, and you can manage them in a basic way. A future version will add more.
- Fixed media attachments. Files that are not very small now attach correctly.
- Fixed the handling of JSON responses from some servers.
- Fixed a rare problem that closed the compose dialog.
- You can type more than the character limit again. Fedra plays a warning sound when you do.
- Post statistics now use the correct plural. For example, you now hear "1 reply", not "1 replies".
- `Shift+F10` or the Applications key on a post now opens a menu of post actions.
- Replies now show in threads correctly.
- The mentions dialog now includes users that your instance doesn't know yet.
- The open user dialog now shows all the user names from your current timeline.
- When you close a timeline, Fedra now announces the name of the new timeline before its contents.

### Version 0.1.1
- Added the post language setting.
- Error messages now include a short form of API errors.
- Fixed `Delete` not closing timelines when the list had the keyboard focus.
- Better default settings for new installations.
- Screen readers now read less when you open the compose dialog.
- The compose dialog now enforces the character limit of the instance.
- `Ctrl+1` through `Ctrl+9` now announce the name of the timeline, as the arrow keys do.
- The reply dialog title now shows the correct character count when it first gets the focus.
- There is now one key to close a timeline: `Ctrl+W` in Normal mode, or `Backspace` in Quick Action Keys mode.
- Updated the readme and cleaned up the code.
- When you close a timeline, Fedra now announces the new timeline.

### Version 0.1.0
- The first release of Fedra, a Mastodon client for Windows.
