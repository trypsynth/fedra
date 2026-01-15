#![windows_subsystem = "windows"]

mod auth;
mod config;
mod mastodon;

use url::Url;
use wxdragon::prelude::*;

fn parse_instance_url(value: &str) -> Option<Url> {
	let trimmed = value.trim();
	if trimmed.is_empty() {
		return None;
	}
	let candidate = if trimmed.contains("://") { trimmed.to_string() } else { format!("https://{}", trimmed) };
	let mut url = Url::parse(&candidate).ok()?;
	if url.host_str().is_none() {
		return None;
	}
	if url.scheme() != "https" && url.scheme() != "http" {
		return None;
	}
	url.set_fragment(None);
	url.set_query(None);
	url.set_path("/");
	Some(url)
}

fn prompt_for_instance(frame: &Frame) -> Option<Url> {
	loop {
		let dialog =
			TextEntryDialog::builder(frame, "Enter your Mastodon instance", "Add Account")
				.with_style(TextEntryDialogStyle::Default | TextEntryDialogStyle::ProcessEnter)
				.build();
		let result = dialog.show_modal();
		if result != ID_OK {
			dialog.destroy();
			return None;
		}
		let value = dialog.get_value().unwrap_or_default();
		dialog.destroy();
		if let Some(instance) = parse_instance_url(&value) {
			return Some(instance);
		}
		let message = MessageDialog::builder(frame, "Please enter a valid instance URL.", "Invalid Instance")
			.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconWarning)
			.build();
		message.show_modal();
	}
}

fn prompt_for_oauth_code(frame: &Frame, instance: &Url) -> Option<String> {
	let dialog = TextEntryDialog::builder(
		frame,
		&format!("After authorizing Fedra on {}, paste the code here.", instance.host_str().unwrap_or("your instance")),
		"Authorize Fedra",
	)
	.with_style(TextEntryDialogStyle::Default | TextEntryDialogStyle::ProcessEnter)
	.build();
	let result = dialog.show_modal();
	if result != ID_OK {
		dialog.destroy();
		return None;
	}
	let value = dialog.get_value().unwrap_or_default();
	dialog.destroy();
	let trimmed = value.trim();
	if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

fn show_error(frame: &Frame, message: &str) {
	let dialog = MessageDialog::builder(frame, message, "Fedra")
		.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
		.build();
	dialog.show_modal();
}

fn prompt_for_access_token(frame: &Frame, instance: &Url) -> Option<String> {
	let dialog = TextEntryDialog::builder(
		frame,
		&format!(
			"OAuth failed. Create an access token on {} and paste it here.",
			instance.host_str().unwrap_or("your instance")
		),
		"Manual Access Token",
	)
	.with_style(TextEntryDialogStyle::Default | TextEntryDialogStyle::ProcessEnter)
	.build();
	let result = dialog.show_modal();
	if result != ID_OK {
		dialog.destroy();
		return None;
	}
	let value = dialog.get_value().unwrap_or_default();
	dialog.destroy();
	let trimmed = value.trim();
	if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

fn main() {
	let _ = wxdragon::main(|_| {
		let frame = Frame::builder().with_title("Fedra").with_size(Size::new(800, 600)).build();
		wxdragon::app::set_top_window(&frame);
		let panel = Panel::builder(&frame).build();
		let sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let timelines_label = StaticText::builder(&panel).with_label("Timelines").build();
		let timelines = ListBox::builder(&panel)
			.with_choices(vec!["Home".to_string(), "Local".to_string(), "Federated".to_string()])
			.build();
		timelines.set_selection(0, true);
		let timeline_content = ListBox::builder(&panel).build();
		let timelines_sizer = BoxSizer::builder(Orientation::Vertical).build();
		timelines_sizer.add(&timelines_label, 0, SizerFlag::All, 8);
		timelines_sizer.add(
			&timelines,
			1,
			SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
			8,
		);
		sizer.add_sizer(&timelines_sizer, 1, SizerFlag::Expand, 0);
		sizer.add(&timeline_content, 3, SizerFlag::Expand | SizerFlag::All, 8);
		panel.set_sizer(sizer, true);
		let frame_sizer = BoxSizer::builder(Orientation::Vertical).build();
		frame_sizer.add(&panel, 1, SizerFlag::Expand | SizerFlag::All, 0);
		frame.set_sizer(frame_sizer, true);
		let store = config::ConfigStore::new();
		let mut config = store.load();
		if config.accounts.is_empty() {
			let instance_url = prompt_for_instance(&frame);
			let instance_url = match instance_url {
				Some(value) => value,
				None => {
					frame.close(true);
					return;
				}
			};
			let client = match mastodon::MastodonClient::new(instance_url.clone()) {
				Ok(client) => client,
				Err(_) => {
					show_error(&frame, "Could not create a Mastodon client for that instance.");
					frame.close(true);
					return;
				}
			};
			let mut account = config::Account::new(instance_url.to_string());
			let mut oauth_ok = false;
			if let Ok(credentials) = auth::oauth_with_local_listener(&client, "Fedra") {
				account.access_token = Some(credentials.access_token);
				account.client_id = Some(credentials.client_id);
				account.client_secret = Some(credentials.client_secret);
				oauth_ok = true;
			}
			if !oauth_ok {
				let credentials = match client.register_app("Fedra", auth::OOB_REDIRECT_URI) {
					Ok(credentials) => credentials,
					Err(_) => {
						show_error(&frame, "Failed to register the app with your instance.");
						frame.close(true);
						return;
					}
				};
				let authorize_url = match client.build_authorize_url(&credentials, auth::OOB_REDIRECT_URI) {
					Ok(url) => url,
					Err(_) => {
						show_error(&frame, "Failed to build the authorization URL.");
						frame.close(true);
						return;
					}
				};
				let _ = webbrowser::open(authorize_url.as_str());
				let code = match prompt_for_oauth_code(&frame, &instance_url) {
					Some(code) => code,
					None => {
						frame.close(true);
						return;
					}
				};
				let access_token = match client.exchange_token(&credentials, &code, auth::OOB_REDIRECT_URI) {
					Ok(token) => token,
					Err(_) => {
						show_error(&frame, "Failed to exchange the authorization code for a token.");
						frame.close(true);
						return;
					}
				};
				account.access_token = Some(access_token);
				account.client_id = Some(credentials.client_id);
				account.client_secret = Some(credentials.client_secret);
				oauth_ok = true;
			}
			if !oauth_ok {
				show_error(&frame, "OAuth failed. Falling back to manual access token.");
				let token = match prompt_for_access_token(&frame, &instance_url) {
					Some(token) => token,
					None => {
						frame.close(true);
						return;
					}
				};
				account.access_token = Some(token);
			}
			config.accounts.push(account);
			let _ = store.save(&config);
		}
		frame.show(true);
		frame.centre();
	});
}
