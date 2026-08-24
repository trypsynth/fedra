//! Server-side filter management.

use anyhow::Result;

use crate::mastodon::{Filter, FilterAction, FilterContext, FilterKeyword, MastodonClient};

impl MastodonClient {
	pub fn get_filters(&self, access_token: &str) -> Result<Vec<Filter>> {
		let url = self.base_url.join("api/v2/filters")?;
		self.get_json(access_token, url, "fetch filters")
	}

	pub fn create_filter(
		&self,
		access_token: &str,
		title: &str,
		contexts: &[FilterContext],
		action: &FilterAction,
		keywords: &[(String, bool)], // (keyword, whole_word)
		expires_in: Option<u32>,
	) -> Result<Filter> {
		let url = self.base_url.join("api/v2/filters")?;
		let mut params = vec![
			("title".to_string(), title.to_string()),
			("filter_action".to_string(), action.to_string().to_lowercase()),
		];
		for context in contexts {
			params.push(("context[]".to_string(), format!("{context:?}").to_lowercase()));
		}
		for (i, (keyword, whole_word)) in keywords.iter().enumerate() {
			params.push((format!("keywords_attributes[{i}][keyword]"), keyword.clone()));
			params.push((format!("keywords_attributes[{i}][whole_word]"), whole_word.to_string()));
		}
		if let Some(expires_in) = expires_in {
			params.push(("expires_in".to_string(), expires_in.to_string()));
		}

		Self::send_json(self.http.post(url).bearer_auth(access_token).form(&params), "create filter")
	}

	pub fn update_filter(
		&self,
		access_token: &str,
		id: &str,
		title: &str,
		contexts: &[FilterContext],
		action: &FilterAction,
		keywords_attributes: &[(&str, &str, bool, bool)], // (id, keyword, whole_word, destroy)
		expires_in: Option<u32>,
	) -> Result<Filter> {
		let url = self.base_url.join(&format!("api/v2/filters/{id}"))?;
		let mut params = vec![
			("title".to_string(), title.to_string()),
			("filter_action".to_string(), action.to_string().to_lowercase()),
		];
		for context in contexts {
			params.push(("context[]".to_string(), format!("{context:?}").to_lowercase()));
		}
		for (i, (keyword_id, keyword, whole_word, destroy)) in keywords_attributes.iter().enumerate() {
			if !keyword_id.is_empty() {
				params.push((format!("keywords_attributes[{i}][id]"), (*keyword_id).to_string()));
			}
			params.push((format!("keywords_attributes[{i}][keyword]"), (*keyword).to_string()));
			params.push((format!("keywords_attributes[{i}][whole_word]"), whole_word.to_string()));
			if *destroy {
				params.push((format!("keywords_attributes[{i}][_destroy]"), "true".to_string()));
			}
		}
		if let Some(expires_in) = expires_in {
			params.push(("expires_in".to_string(), expires_in.to_string()));
		}

		Self::send_json(self.http.put(url).bearer_auth(access_token).form(&params), "update filter")
	}

	pub fn delete_filter(&self, access_token: &str, id: &str) -> Result<()> {
		let url = self.base_url.join(&format!("api/v2/filters/{id}"))?;
		self.delete_empty(access_token, url, "delete filter")
	}

	pub fn add_filter_keyword(
		&self,
		access_token: &str,
		filter_id: &str,
		keyword: &str,
		whole_word: bool,
	) -> Result<FilterKeyword> {
		let url = self.base_url.join(&format!("api/v2/filters/{filter_id}/keywords"))?;
		let form = [("keyword", keyword), ("whole_word", if whole_word { "true" } else { "false" })];
		Self::send_json(self.http.post(url).bearer_auth(access_token).form(&form), "add filter keyword")
	}

	pub fn delete_filter_keyword(&self, access_token: &str, keyword_id: &str) -> Result<()> {
		let url = self.base_url.join(&format!("api/v2/filters/keywords/{keyword_id}"))?;
		self.delete_empty(access_token, url, "delete filter keyword")
	}
}
