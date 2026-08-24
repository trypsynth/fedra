//! List management and membership.

use anyhow::Result;

use crate::mastodon::{Account, List, MastodonClient, list::list_form};

impl MastodonClient {
	pub fn get_lists(&self, access_token: &str) -> Result<Vec<List>> {
		let url = self.base_url.join("api/v1/lists")?;
		self.get_json(access_token, url, "fetch lists")
	}

	pub fn create_list(&self, access_token: &str, title: &str, replies_policy: &str, exclusive: bool) -> Result<List> {
		let url = self.base_url.join("api/v1/lists")?;
		let form = list_form(title, replies_policy, exclusive);
		Self::send_json(self.http.post(url).bearer_auth(access_token).form(&form), "create list")
	}

	pub fn update_list(
		&self,
		access_token: &str,
		id: &str,
		title: &str,
		replies_policy: &str,
		exclusive: bool,
	) -> Result<List> {
		let url = self.base_url.join(&format!("api/v1/lists/{id}"))?;
		let form = list_form(title, replies_policy, exclusive);
		Self::send_json(self.http.put(url).bearer_auth(access_token).form(&form), "update list")
	}

	pub fn delete_list(&self, access_token: &str, id: &str) -> Result<()> {
		let url = self.base_url.join(&format!("api/v1/lists/{id}"))?;
		self.delete_empty(access_token, url, "delete list")
	}

	pub fn get_list_accounts(&self, access_token: &str, list_id: &str) -> Result<Vec<Account>> {
		let mut url = self.base_url.join(&format!("api/v1/lists/{list_id}/accounts"))?;
		url.query_pairs_mut().append_pair("limit", "0");
		self.get_json(access_token, url, "fetch list accounts")
	}

	pub fn add_list_accounts(&self, access_token: &str, list_id: &str, account_ids: &[String]) -> Result<()> {
		let url = self.base_url.join(&format!("api/v1/lists/{list_id}/accounts"))?;
		let mut params = Vec::new();
		for id in account_ids {
			params.push(("account_ids[]", id));
		}
		Self::send_empty(self.http.post(url).bearer_auth(access_token).form(&params), "add accounts to list")
	}

	pub fn remove_list_accounts(&self, access_token: &str, list_id: &str, account_ids: &[String]) -> Result<()> {
		let mut url = self.base_url.join(&format!("api/v1/lists/{list_id}/accounts"))?;
		{
			let mut query = url.query_pairs_mut();
			for id in account_ids {
				query.append_pair("account_ids[]", id);
			}
		}
		self.delete_empty(access_token, url, "remove accounts from list")
	}
}
