use std::path::Path;

use crate::{
	AppState,
	audio::AudioOutput,
	config::NotificationPreference,
	mastodon::{Notification, Status},
	timeline::TimelineType,
	ui::app_shell::AppShell,
};

#[cfg(windows)]
fn show(app_shell: Option<&AppShell>, title: &str, body: &str) {
	if let Some(app_shell) = app_shell {
		// wxICON_INFORMATION = 0x00000002
		app_shell.taskbar.show_balloon(title, body, 5000, 0x0000_0002, None);
	}
}

#[cfg(not(windows))]
fn show(_app_shell: Option<&AppShell>, title: &str, body: &str) {
	use std::cell::RefCell;

	use wxdragon::prelude::{NotificationMessage, TIMEOUT_AUTO};
	// Kept until the next one, since dropping a notification takes it off the screen.
	thread_local! {
		static LAST: RefCell<Option<NotificationMessage>> = const { RefCell::new(None) };
	}
	if let Ok(message) = NotificationMessage::builder().with_title(title).with_message(body).build() {
		message.show(TIMEOUT_AUTO);
		LAST.with_borrow_mut(|last| *last = Some(message));
	}
}

pub fn show_notification(app_shell: Option<&AppShell>, notification: &Notification) {
	show(app_shell, notification.account.display_name_or_username(), &notification.simple_display());
}

const DEFAULT_SOUND: &[u8] = include_bytes!("../sounds/boop.mp3");

/// Plays `custom`, or the default notification sound when there's none.
pub fn play_sound(output: Option<&AudioOutput>, custom: Option<&Path>) {
	let Some(output) = output else { return };
	match custom {
		Some(path) => crate::audio::play_once(output, path),
		None => crate::audio::play_bytes_once(output, DEFAULT_SOUND),
	}
}

/// Tells the user that `count` message requests are waiting, in the chosen notification style.
pub fn notify_message_requests(state: &AppState, count: u64) {
	match state.config.notification_preference {
		NotificationPreference::Classic => {
			let body = if count == 1 {
				"1 message request is waiting".to_string()
			} else {
				format!("{count} message requests are waiting")
			};
			show(state.app_shell.as_deref(), "Message requests", &body);
		}
		NotificationPreference::SoundOnly => {
			play_sound(state.notification_sound.as_ref(), None);
		}
		NotificationPreference::Disabled => {}
	}
}

/// Alerts once for a batch of new posts in timelines the user asked to be notified about, each
/// paired with its timeline.
pub fn notify_new_posts(state: &AppState, posts: &[(TimelineType, Status)]) {
	let Some((timeline, status)) = posts.first() else { return };
	match state.config.notification_preference {
		NotificationPreference::Classic => {
			let app_shell = state.app_shell.as_deref();
			let timeline_name = timeline.display_name();
			if posts.len() == 1 {
				let title = format!("{} in {timeline_name}", status.account.display_name_or_username());
				show(app_shell, &title, &status.simple_display());
			} else {
				show(app_shell, &format!("{} new posts", posts.len()), &format!("In {timeline_name}"));
			}
		}
		NotificationPreference::SoundOnly => {
			let mut played = Vec::new();
			for (timeline, _) in posts {
				if !played.contains(&timeline) {
					played.push(timeline);
					let custom = state.active_account().and_then(|account| account.timeline_sound(timeline));
					play_sound(state.notification_sound.as_ref(), custom);
				}
			}
		}
		NotificationPreference::Disabled => {}
	}
}
