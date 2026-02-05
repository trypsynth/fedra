use crate::mastodon::Notification;

pub fn show_notification(app_shell: &crate::ui::app_shell::AppShell, notification: &Notification) {
	let title = notification.account.display_name_or_username();
	let body = notification.simple_display();

	// wxICON_INFORMATION = 0x00000002
	app_shell.taskbar.show_balloon(title, &body, 5000, 0x00000002, None);
}
