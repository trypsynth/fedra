use reqwest::{Url, blocking::Client};
use serde::Deserialize;

pub const DEFAULT_SCOPES: &str = "read write follow";

#[derive(Debug, Clone)]
pub struct MastodonClient {
	base_url: Url,
	http: Client,
}

#[derive(Debug, Clone)]
pub struct AppCredentials {
	pub client_id: String,
	pub client_secret: String,
}

#[derive(Debug)]
pub enum MastodonError {
	Url,
	Http,
}

impl From<url::ParseError> for MastodonError {
	fn from(value: url::ParseError) -> Self {
		let _ = value;
		Self::Url
	}
}

impl From<reqwest::Error> for MastodonError {
	fn from(value: reqwest::Error) -> Self {
		let _ = value;
		Self::Http
	}
}

impl MastodonClient {
	pub fn new(base_url: Url) -> Result<Self, MastodonError> {
		let http = Client::builder().user_agent("Fedra/0.1").build()?;
		Ok(Self { base_url, http })
	}

	pub fn register_app(&self, app_name: &str, redirect_uri: &str) -> Result<AppCredentials, MastodonError> {
		let url = self.base_url.join("api/v1/apps")?;
		let response = self
			.http
			.post(url)
			.form(&[
				("client_name", app_name),
				("redirect_uris", redirect_uri),
				("scopes", DEFAULT_SCOPES),
				("website", ""),
			])
			.send()?
			.error_for_status()?;
		let payload: RegisterAppResponse = response.json()?;
		Ok(AppCredentials { client_id: payload.client_id, client_secret: payload.client_secret })
	}

	pub fn build_authorize_url(&self, credentials: &AppCredentials, redirect_uri: &str) -> Result<Url, MastodonError> {
		let mut url = self.base_url.join("oauth/authorize")?;
		url.query_pairs_mut()
			.append_pair("client_id", &credentials.client_id)
			.append_pair("redirect_uri", redirect_uri)
			.append_pair("response_type", "code")
			.append_pair("scope", DEFAULT_SCOPES);
		Ok(url)
	}

	pub fn exchange_token(
		&self,
		credentials: &AppCredentials,
		code: &str,
		redirect_uri: &str,
	) -> Result<String, MastodonError> {
		let url = self.base_url.join("oauth/token")?;
		let response = self
			.http
			.post(url)
			.form(&[
				("client_id", credentials.client_id.as_str()),
				("client_secret", credentials.client_secret.as_str()),
				("redirect_uri", redirect_uri),
				("grant_type", "authorization_code"),
				("code", code),
				("scope", DEFAULT_SCOPES),
			])
			.send()?
			.error_for_status()?;
		let payload: TokenResponse = response.json()?;
		Ok(payload.access_token)
	}
}

#[derive(Debug, Deserialize)]
struct RegisterAppResponse {
	client_id: String,
	client_secret: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
	access_token: String,
}
