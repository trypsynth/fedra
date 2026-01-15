#![windows_subsystem = "windows"]

mod auth;
mod config;
mod error;
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
	if url.host_str().is_none() || !matches!(url.scheme(), "https" | "http") {
		return None;
	}
	url.set_fragment(None);
	url.set_query(None);
	url.set_path("/");
	Some(url)
}

fn prompt_for_instance(frame: &Frame) -> Option<Url> {
	loop {
		let dialog = TextEntryDialog::builder(frame, "Enter your Mastodon instance", "Add Account")
			.with_style(TextEntryDialogStyle::Default | TextEntryDialogStyle::ProcessEnter)
			.build();
		if dialog.show_modal() != ID_OK {
			dialog.destroy();
			return None;
		}
		let value = dialog.get_value().unwrap_or_default();
		dialog.destroy();
		if let Some(instance) = parse_instance_url(&value) {
			return Some(instance);
		}
		show_warning(frame, "Please enter a valid instance URL.", "Invalid Instance");
	}
}

fn prompt_for_oauth_code(frame: &Frame, instance: &Url) -> Option<String> {
	let message =
		format!("After authorizing Fedra on {}, paste the code here.", instance.host_str().unwrap_or("your instance"));
	prompt_text(frame, &message, "Authorize Fedra")
}

fn prompt_for_access_token(frame: &Frame, instance: &Url) -> Option<String> {
	let message = format!(
		"OAuth failed. Create an access token on {} and paste it here.",
		instance.host_str().unwrap_or("your instance")
	);
	prompt_text(frame, &message, "Manual Access Token")
}

fn prompt_text(frame: &Frame, message: &str, title: &str) -> Option<String> {
	let dialog = TextEntryDialog::builder(frame, message, title)
		.with_style(TextEntryDialogStyle::Default | TextEntryDialogStyle::ProcessEnter)
		.build();
	if dialog.show_modal() != ID_OK {
		dialog.destroy();
		return None;
	}
	let value = dialog.get_value().unwrap_or_default();
	dialog.destroy();
	let trimmed = value.trim();
	if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

fn show_error(frame: &Frame, err: &anyhow::Error) {
	let dialog = MessageDialog::builder(frame, error::user_message(err), "Fedra")
		.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
		.build();
	dialog.show_modal();
}

fn show_warning(frame: &Frame, message: &str, title: &str) {
	let dialog = MessageDialog::builder(frame, message, title)
		.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconWarning)
		.build();
	dialog.show_modal();
}

fn setup_new_account(frame: &Frame) -> Option<config::Account> {
	let instance_url = prompt_for_instance(frame)?;
	let client = match mastodon::MastodonClient::new(instance_url.clone()) {
		Ok(client) => client,
		Err(err) => {
			show_error(frame, &err);
			return None;
		}
	};
	let mut account = config::Account::new(instance_url.to_string());
	// Try OAuth with local listener
	if let Ok(result) = auth::oauth_with_local_listener(&client, "Fedra") {
		account.access_token = Some(result.access_token);
		account.client_id = Some(result.client_id);
		account.client_secret = Some(result.client_secret);
		return Some(account);
	}
	// Fall back to out-of-band OAuth
	if let Some(acc) = try_oob_oauth(frame, &client, &instance_url, &mut account) {
		return Some(acc);
	}
	// Fall back to manual token entry
	let token = prompt_for_access_token(frame, &instance_url)?;
	account.access_token = Some(token);
	Some(account)
}

fn try_oob_oauth(
	frame: &Frame,
	client: &mastodon::MastodonClient,
	instance_url: &Url,
	account: &mut config::Account,
) -> Option<config::Account> {
	let credentials = match client.register_app("Fedra", auth::OOB_REDIRECT_URI) {
		Ok(creds) => creds,
		Err(err) => {
			show_error(frame, &err);
			return None;
		}
	};
	let authorize_url = match client.build_authorize_url(&credentials, auth::OOB_REDIRECT_URI) {
		Ok(url) => url,
		Err(err) => {
			show_error(frame, &err);
			return None;
		}
	};
	let _ = webbrowser::open(authorize_url.as_str());
	let code = prompt_for_oauth_code(frame, instance_url)?;
	let access_token = match client.exchange_token(&credentials, &code, auth::OOB_REDIRECT_URI) {
		Ok(token) => token,
		Err(err) => {
			show_error(frame, &err);
			return None;
		}
	};
	account.access_token = Some(access_token);
	account.client_id = Some(credentials.client_id);
	account.client_secret = Some(credentials.client_secret);
	Some(account.clone())
}

fn build_main_ui(frame: &Frame) {
	let panel = Panel::builder(frame).build();
	let sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let timelines_label = StaticText::builder(&panel).with_label("Timelines").build();
	let timelines = ListBox::builder(&panel)
		.with_choices(vec!["Home".to_string(), "Local".to_string(), "Federated".to_string()])
		.build();
	timelines.set_selection(0, true);
	let timeline_content = ListBox::builder(&panel).build();
	let timelines_sizer = BoxSizer::builder(Orientation::Vertical).build();
	timelines_sizer.add(&timelines_label, 0, SizerFlag::All, 8);
	timelines_sizer.add(&timelines, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 8);
	sizer.add_sizer(&timelines_sizer, 1, SizerFlag::Expand, 0);
	sizer.add(&timeline_content, 3, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(sizer, true);
	let frame_sizer = BoxSizer::builder(Orientation::Vertical).build();
	frame_sizer.add(&panel, 1, SizerFlag::Expand | SizerFlag::All, 0);
	frame.set_sizer(frame_sizer, true);
}

fn main() {
	let _ = wxdragon::main(|_| {
		let frame = Frame::builder().with_title("Fedra").with_size(Size::new(800, 600)).build();
		wxdragon::app::set_top_window(&frame);
		build_main_ui(&frame);
		let store = config::ConfigStore::new();
		let mut config = store.load();
		if config.accounts.is_empty() {
			match setup_new_account(&frame) {
				Some(account) => {
					config.accounts.push(account);
					let _ = store.save(&config);
				}
				None => {
					frame.close(true);
					return;
				}
			}
		}
		frame.show(true);
		frame.centre();
	});
}
