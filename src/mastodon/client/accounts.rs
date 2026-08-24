//! Account lookup, relationships, and profile updates.

use anyhow::{Context, Result};
use reqwest::{Url, blocking::multipart};

use crate::mastodon::{Account, MastodonClient, Relationship};

impl MastodonClient {
	pub fn get_account(&self, access_token: &str, account_id: &str) -> Result<Account> {
		let url = self.base_url.join(&format!("api/v1/accounts/{account_id}"))?;
		self.get_json(access_token, url, "fetch account")
	}

	pub fn lookup_account(&self, access_token: &str, acct: &str) -> Result<Account> {
		let mut url = self.base_url.join("api/v1/accounts/lookup")?;
		url.query_pairs_mut().append_pair("acct", acct);
		self.get_json(access_token, url, "lookup account")
	}

	fn fetch_accounts_page(
		&self,
		base_url: Url,
		access_token: Option<&str>,
		max_id: Option<&str>,
	) -> Result<(Vec<Account>, Option<String>)> {
		let mut url = base_url;
		{
			let mut query = url.query_pairs_mut();
			query.append_pair("limit", "80");
			if let Some(id) = max_id {
				query.append_pair("max_id", id);
			}
		}
		let mut req = self.http.get(url);
		if let Some(token) = access_token {
			req = req.bearer_auth(token);
		}
		let response = req.send()?.error_for_status()?;
		let next_max_id = Self::next_max_id(&response);
		let accounts: Vec<Account> = response.json()?;
		Ok((accounts, next_max_id))
	}

	fn fetch_all_accounts(&self, base_url: Url, access_token: Option<&str>) -> Result<Vec<Account>> {
		let mut all_accounts = Vec::new();
		let mut max_id: Option<String> = None;
		loop {
			let (accounts, next) = self.fetch_accounts_page(base_url.clone(), access_token, max_id.as_deref())?;
			let done = accounts.is_empty() || next.is_none();
			all_accounts.extend(accounts);
			if done {
				break;
			}
			max_id = next;
		}
		Ok(all_accounts)
	}

	pub fn get_followers_page(
		&self,
		access_token: &str,
		account_id: &str,
		max_id: Option<&str>,
	) -> Result<(Vec<Account>, Option<String>)> {
		let url = self.base_url.join(&format!("api/v1/accounts/{account_id}/followers"))?;
		self.fetch_accounts_page(url, Some(access_token), max_id).context("Failed to fetch followers")
	}

	pub fn get_following_page(
		&self,
		access_token: &str,
		account_id: &str,
		max_id: Option<&str>,
	) -> Result<(Vec<Account>, Option<String>)> {
		let url = self.base_url.join(&format!("api/v1/accounts/{account_id}/following"))?;
		self.fetch_accounts_page(url, Some(access_token), max_id).context("Failed to fetch following")
	}

	pub fn get_remote_followers(&self, acct: &str) -> Result<Vec<Account>> {
		let (base_url, remote_id) = self.resolve_remote_account(acct)?;
		let url = base_url.join(&format!("api/v1/accounts/{remote_id}/followers"))?;
		self.fetch_all_accounts(url, None).context("Failed to fetch remote followers")
	}

	pub fn get_remote_following(&self, acct: &str) -> Result<Vec<Account>> {
		let (base_url, remote_id) = self.resolve_remote_account(acct)?;
		let url = base_url.join(&format!("api/v1/accounts/{remote_id}/following"))?;
		self.fetch_all_accounts(url, None).context("Failed to fetch remote following")
	}

	fn resolve_remote_account(&self, acct: &str) -> Result<(Url, String)> {
		let domain = acct.split('@').nth(1).ok_or_else(|| anyhow::anyhow!("Invalid remote acct: {acct}"))?;
		if domain.is_empty() {
			return Err(anyhow::anyhow!("Invalid remote acct: {acct}"));
		}
		let base_url = Url::parse(&format!("https://{domain}/"))?;
		let mut lookup_url = base_url.join("api/v1/accounts/lookup")?;
		lookup_url.query_pairs_mut().append_pair("acct", acct);
		let account: Account = self
			.http
			.get(lookup_url)
			.send()
			.context("Failed to lookup account on remote instance")?
			.error_for_status()
			.context("Could not find this account on their home instance")?
			.json()
			.context("Invalid account response from remote instance")?;
		Ok((base_url, account.id))
	}

	pub fn get_relationships(&self, access_token: &str, account_ids: &[String]) -> Result<Vec<Relationship>> {
		let mut url = self.base_url.join("api/v1/accounts/relationships")?;
		{
			let mut query = url.query_pairs_mut();
			for id in account_ids {
				query.append_pair("id[]", id);
			}
		}
		self.get_json(access_token, url, "fetch relationships")
	}

	#[allow(dead_code)]
	pub fn follow_account(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		self.follow_account_with_options(access_token, account_id, true)
	}

	pub fn follow_account_with_options(
		&self,
		access_token: &str,
		account_id: &str,
		reblogs: bool,
	) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/accounts/{account_id}/follow"))?;
		let form = [("reblogs", if reblogs { "true" } else { "false" })];
		Self::send_json(self.http.post(url).bearer_auth(access_token).form(&form), "follow account")
	}

	pub fn unfollow_account(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/accounts/{account_id}/unfollow"))?;
		self.post_json(access_token, url, "unfollow account")
	}

	pub fn authorize_follow_request(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/follow_requests/{account_id}/authorize"))?;
		self.post_json(access_token, url, "authorize follow request")
	}

	pub fn reject_follow_request(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/follow_requests/{account_id}/reject"))?;
		self.post_json(access_token, url, "reject follow request")
	}

	pub fn block_account(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/accounts/{account_id}/block"))?;
		self.post_json(access_token, url, "block account")
	}

	pub fn unblock_account(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/accounts/{account_id}/unblock"))?;
		self.post_json(access_token, url, "unblock account")
	}

	pub fn mute_account(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/accounts/{account_id}/mute"))?;
		self.post_json(access_token, url, "mute account")
	}

	pub fn unmute_account(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/accounts/{account_id}/unmute"))?;
		self.post_json(access_token, url, "unmute account")
	}

	pub fn update_credentials(
		&self,
		access_token: &str,
		display_name: Option<&str>,
		note: Option<&str>,
		avatar: Option<&str>,
		header: Option<&str>,
		locked: Option<bool>,
		bot: Option<bool>,
		discoverable: Option<bool>,
		fields_attributes: Option<&[(String, String)]>,
		source_privacy: Option<&str>,
		source_sensitive: Option<bool>,
		source_language: Option<&str>,
	) -> Result<Account> {
		let url = self.base_url.join("api/v1/accounts/update_credentials")?;
		let mut form = multipart::Form::new();

		if let Some(v) = display_name {
			form = form.text("display_name", v.to_string());
		}
		if let Some(v) = note {
			form = form.text("note", v.to_string());
		}
		if let Some(v) = avatar {
			let part = multipart::Part::file(v).context("Failed to read avatar file")?;
			form = form.part("avatar", part);
		}
		if let Some(v) = header {
			let part = multipart::Part::file(v).context("Failed to read header file")?;
			form = form.part("header", part);
		}
		if let Some(v) = locked {
			form = form.text("locked", v.to_string());
		}
		if let Some(v) = bot {
			form = form.text("bot", v.to_string());
		}
		if let Some(v) = discoverable {
			form = form.text("discoverable", v.to_string());
		}
		if let Some(fields) = fields_attributes {
			for (i, (name, value)) in fields.iter().enumerate() {
				form = form.text(format!("fields_attributes[{i}][name]"), name.clone());
				form = form.text(format!("fields_attributes[{i}][value]"), value.clone());
			}
		}
		if let Some(v) = source_privacy {
			form = form.text("source[privacy]", v.to_string());
		}
		if let Some(v) = source_sensitive {
			form = form.text("source[sensitive]", v.to_string());
		}
		if let Some(v) = source_language {
			form = form.text("source[language]", v.to_string());
		}

		Self::send_json(self.http.patch(url).bearer_auth(access_token).multipart(form), "update credentials")
	}
}
