//! App registration, the OAuth flow, and credential verification.

use anyhow::Result;
use reqwest::Url;
use serde::Deserialize;

use crate::mastodon::{Account, AppCredentials, DEFAULT_SCOPES, MastodonClient};

#[derive(Debug, Deserialize)]
struct RegisterAppResponse {
	client_id: String,
	client_secret: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
	access_token: String,
}

impl MastodonClient {
	pub fn register_app(&self, app_name: &str, redirect_uri: &str) -> Result<AppCredentials> {
		let url = self.base_url.join("api/v1/apps")?;
		let request = self.http.post(url).form(&[
			("client_name", app_name),
			("redirect_uris", redirect_uri),
			("scopes", DEFAULT_SCOPES),
			("website", ""),
		]);
		let payload: RegisterAppResponse = Self::send_json(request, "register app with instance")?;
		Ok(AppCredentials { client_id: payload.client_id, client_secret: payload.client_secret })
	}

	pub fn build_authorize_url(&self, credentials: &AppCredentials, redirect_uri: &str) -> Result<Url> {
		let mut url = self.base_url.join("oauth/authorize")?;
		url.query_pairs_mut()
			.append_pair("client_id", &credentials.client_id)
			.append_pair("redirect_uri", redirect_uri)
			.append_pair("response_type", "code")
			.append_pair("scope", DEFAULT_SCOPES);
		Ok(url)
	}

	pub fn exchange_token(&self, credentials: &AppCredentials, code: &str, redirect_uri: &str) -> Result<String> {
		let url = self.base_url.join("oauth/token")?;
		let request = self.http.post(url).form(&[
			("client_id", credentials.client_id.as_str()),
			("client_secret", credentials.client_secret.as_str()),
			("redirect_uri", redirect_uri),
			("grant_type", "authorization_code"),
			("code", code),
			("scope", DEFAULT_SCOPES),
		]);
		let payload: TokenResponse = Self::send_json(request, "exchange token")?;
		Ok(payload.access_token)
	}

	pub fn verify_credentials(&self, access_token: &str) -> Result<Account> {
		let url = self.base_url.join("api/v1/accounts/verify_credentials")?;
		self.get_json(access_token, url, "verify credentials")
	}
}
