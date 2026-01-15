pub use anyhow::{Context, Result};

pub fn user_message(err: &anyhow::Error) -> &'static str {
	let msg = err.to_string();
	if msg.contains("network") || msg.contains("HTTP") || msg.contains("request") {
		return "A network error occurred. Please check your connection.";
	}
	if msg.contains("URL") || msg.contains("url") {
		return "The instance URL is invalid.";
	}
	if msg.contains("register") {
		return "Failed to register the app with your instance.";
	}
	if msg.contains("authorization") || msg.contains("authorize") {
		return "Failed during the authorization process.";
	}
	if msg.contains("token") {
		return "Failed to obtain an access token.";
	}
	if msg.contains("timeout") {
		return "The operation timed out. Please try again.";
	}
	if msg.contains("browser") {
		return "Could not open your web browser.";
	}
	if msg.contains("listener") || msg.contains("bind") {
		return "Could not start the OAuth callback listener.";
	}
	"An unexpected error occurred."
}
