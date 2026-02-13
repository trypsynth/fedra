# Fedra User Manual

[Fedra](https://github.com/trypsynth/fedra) is a lightweight, fast, and accessible Mastodon client for Windows. It is designed to be completely usable with screen readers and keyboard navigation, providing a seamless social media experience without the bloat.

## System Requirements
Fedra is designed for Windows and supports Windows 10 and Windows 11.

## Features
* Native & Fast: Written in Rust using the wxDragon GUI library for maximum performance and native accessibility.
* Accessible: Built from the ground up for screen reader users, with live region announcements and full keyboard control.
* Multi-Account Support: Manage and switch between multiple Mastodon accounts easily.
* Timeline Management: View Home, Local, Federated, Bookmarks, Favorites, and Notifications timelines. Open multiple user timelines or threads in parallel.
* Streaming: Real-time updates for your timelines.
* Rich Posting: Create posts with media, polls, content warnings, and custom visibility. Reply, edit, and delete posts with ease.
* Keyboard Productivity: Extensive hotkey support, including a "Quick Action" mode for single-key efficiency.
* Tray Integration: Minimize to tray to keep Fedra running in the background.

## Hotkeys
Fedra's user interface was designed specifically with keyboard and screen reader users in mind. As such, every action has an associated hotkey. Below, you'll find a list of all of them, grouped by menu structure.

### General navigation
* Left Arrow: switch to the previous timeline.
* Right Arrow: switch to the next timeline.
* Up Arrow: load more posts (when at the top of a timeline sorted newest to oldest).
* Down Arrow: load more posts (when at the bottom of a timeline sorted newest to oldest).
* Backspace: go back in timeline history (or close current timeline if no history).
* Ctrl + 1-9: switch to the timeline at the specified index (1 through 9).
* Ctrl + W: close the currently focused timeline.
* Ctrl + Backspace: close the currently focused timeline.
* Ctrl + [: switch to the previous account.
* Ctrl + ]: switch to the next account.
* Ctrl + Shift + Q: enable Quick Action Keys mode.

### Options menu
* Ctrl + P: view the profile of the selected post's author.
* Ctrl + ,: configure application settings.

### Post menu
* Ctrl + N: create a new post.
* Ctrl + R: reply to all mentioned users in the selected post.
* Ctrl + Shift + R: reply to the author of the selected post only.
* Ctrl + M: view mentions in the selected post.
* Ctrl + H: view hashtags in the selected post.
* Shift + Enter: open links in the selected post.
* Enter: view the conversation thread for the selected post.
* Ctrl + E: edit the selected post.
* Delete: delete the selected post.
* Ctrl + Shift + F: favorite or unfavorite the selected post.
* Ctrl + Shift + B: boost or unboost the selected post.
* Ctrl + X: toggle content warning expansion for the selected post.

### Timelines menu
* Ctrl + T: view the timeline of the selected post's author.
* Ctrl + U: open a user profile or timeline by entering their handle.
* Ctrl + L: open the local timeline.
* Ctrl + .: load more posts from the server (if available).
* F5: refresh the current timeline.

### Quick Action Keys mode
When enabled (toggle on with Ctrl + Shift + Q), many multi-key shortcuts are replaced with single-key equivalents for efficiency:

* q: disable Quick Action Keys.
* c: create a new post (replaces Ctrl + N).
* r: reply to all (replaces Ctrl + R).
* Ctrl + R: reply to the author of the selected post only (replaces Ctrl + Shift + R).
* f: favorite or unfavorite (replaces Ctrl + Shift + F).
* b: boost or unboost (replaces Ctrl + Shift + B).
* e: edit post (replaces Ctrl + E).
* m: view mentions (replaces Ctrl + M).
* p: view profile (replaces Ctrl + P).
* h: view hashtags (replaces Ctrl + H).
* x: toggle content warning (replaces Ctrl + X).
* . (Period): load more posts (replaces Ctrl + .).
* 1-9: switch to timeline 1-9 (works alongside Ctrl + 1-9).

## Changelog

### Version 0.1.0
* Initial release of the Fedra desktop Mastodon client, currently for Windows only.
