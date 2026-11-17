use std::{
	collections::{HashMap, HashSet},
	fmt, slice,
	sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result};
use chrono::DateTime;
use wx_utils::tasks::{Background, Queue, background_blocking};

use crate::{
	mastodon::{
		Account, Conversation, Filter, FilterAction, FilterContext, MastodonClient, Notification, Poll, PostSubmission,
		Relationship, SearchType, Status, StatusContext, Tag,
	},
	timeline::TimelineType,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationshipAction {
	Follow,
	Unfollow,
	CancelFollowRequest,
	AcceptFollowRequest,
	RejectFollowRequest,
	Block,
	Unblock,
	Mute(MuteOptions),
	Unmute,
	ShowBoosts,
	HideBoosts,
	EnableNotifications,
	DisableNotifications,
	/// Only reported when the current follow state couldn't be looked up.
	ToggleFollow,
}

#[derive(Debug, Clone)]
pub struct PostData {
	pub content: String,
	pub visibility: String,
	pub sensitive: bool,
	pub spoiler_text: Option<String>,
	pub content_type: Option<String>,
	pub language: Option<String>,
	pub media: Vec<MediaUpload>,
	pub poll: Option<PollData>,
	pub quoted_status_id: Option<String>,
	pub scheduled_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProfileUpdate {
	pub display_name: Option<String>,
	pub note: Option<String>,
	pub avatar: Option<String>,
	pub header: Option<String>,
	pub locked: Option<bool>,
	pub bot: Option<bool>,
	pub discoverable: Option<bool>,
	pub fields_attributes: Option<Vec<(String, String)>>,
	pub source: Option<crate::mastodon::Source>,
}

#[derive(Debug, Clone)]
pub struct MediaUpload {
	pub path: String,
	pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub enum EditMedia {
	New(MediaUpload),
	Existing(String),
}

#[derive(Debug, Clone)]
pub struct PollData {
	pub options: Vec<String>,
	pub expires_in: u32,
	pub multiple: bool,
	pub hide_totals: bool,
}

#[derive(Debug)]
pub enum TimelineData {
	Statuses(Vec<Status>, Option<String>),
	Notifications(Vec<Notification>, Option<String>),
	Conversations(Vec<Conversation>, Option<String>),
}

/// Tag mutes live in a filter Fedra owns, so unmuting never touches filters the user made.
const MUTED_TAGS_FILTER: &str = "Muted hashtags";

fn is_muted_tags_filter(filter: &Filter) -> bool {
	filter.title == MUTED_TAGS_FILTER && filter.action == FilterAction::Hide
}

/// An account to find: by its handle, or by the URL of its profile on another instance, which
/// the account's own instance has to look up.
#[derive(Debug, Clone)]
pub enum AccountQuery {
	Handle(String),
	Url(String),
}

impl fmt::Display for AccountQuery {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Handle(text) | Self::Url(text) => f.write_str(text),
		}
	}
}

pub fn find_account(client: &MastodonClient, access_token: &str, query: &AccountQuery) -> Result<Account> {
	match query {
		AccountQuery::Handle(handle) => client.lookup_account(access_token, handle),
		AccountQuery::Url(url) => client
			.search(access_token, url, SearchType::Accounts, Some(1), None)
			.and_then(|mut res| res.accounts.pop().context("Account not found on local instance")),
	}
}

/// The signed-in account, and how many profile fields its instance allows.
pub fn fetch_credentials(client: &MastodonClient, access_token: &str) -> Result<(Account, Option<usize>)> {
	let account = client.verify_credentials(access_token)?;
	Ok((account, client.get_max_profile_fields()))
}

pub fn update_profile(client: &MastodonClient, access_token: &str, update: &ProfileUpdate) -> Result<Account> {
	client.update_credentials(
		access_token,
		update.display_name.as_deref(),
		update.note.as_deref(),
		update.avatar.as_deref(),
		update.header.as_deref(),
		update.locked,
		update.bot,
		update.discoverable,
		update.fields_attributes.as_deref(),
		update.source.as_ref().and_then(|s| s.privacy.as_deref()),
		update.source.as_ref().and_then(|s| s.sensitive),
		update.source.as_ref().and_then(|s| s.language.as_deref()),
	)
}

/// The parts of a post an edit can change.
#[derive(Debug, Clone)]
pub struct StatusEdit {
	pub content: String,
	pub sensitive: bool,
	pub spoiler_text: Option<String>,
	pub language: Option<String>,
	pub media: Vec<EditMedia>,
	pub poll: Option<PollData>,
}

/// A post to reply to or vote in: one on the account's own instance, by its id, or one from
/// elsewhere, by its URL, which is found on the account's instance first.
#[derive(Debug, Clone)]
pub enum Target {
	Id(String),
	Url(String),
}

/// Uploads the post's media, then publishes or schedules it, as a reply to `in_reply_to_id` if
/// there is one.
pub fn submit(
	client: &MastodonClient,
	access_token: &str,
	post: &PostData,
	in_reply_to_id: Option<&str>,
) -> Result<PostSubmission> {
	let media_ids = post
		.media
		.iter()
		.map(|item| client.upload_media(access_token, &item.path, item.description.as_deref()))
		.collect::<Result<Vec<_>>>()?;
	client.post_status_with_media(
		access_token,
		&post.content,
		&post.visibility,
		post.sensitive,
		post.spoiler_text.as_deref(),
		&media_ids,
		post.content_type.as_deref(),
		post.language.as_deref(),
		post.poll.as_ref(),
		in_reply_to_id,
		post.quoted_status_id.as_deref(),
		post.scheduled_at.as_deref(),
	)
}

/// Replies to `target` with `post`.
pub fn reply(client: &MastodonClient, access_token: &str, target: Target, post: &PostData) -> Result<PostSubmission> {
	let in_reply_to_id = match target {
		Target::Id(id) => id,
		Target::Url(url) => resolve_status(client, access_token, &url)?.id,
	};
	submit(client, access_token, post, Some(&in_reply_to_id))
}

/// Uploads the edit's new media, then changes the post to match.
pub fn edit(client: &MastodonClient, access_token: &str, status_id: &str, edit: StatusEdit) -> Result<Status> {
	let media_ids = edit
		.media
		.into_iter()
		.map(|item| match item {
			EditMedia::New(upload) => client.upload_media(access_token, &upload.path, upload.description.as_deref()),
			EditMedia::Existing(id) => Ok(id),
		})
		.collect::<Result<Vec<_>>>()?;
	client.edit_status(
		access_token,
		status_id,
		&edit.content,
		edit.sensitive,
		edit.spoiler_text.as_deref(),
		edit.language.as_deref(),
		&media_ids,
		edit.poll.as_ref(),
	)
}

/// Votes in the poll of `target`, a poll's id or the URL of a post with one.
pub fn vote(client: &MastodonClient, access_token: &str, target: Target, choices: &[usize]) -> Result<Poll> {
	let poll_id = match target {
		Target::Id(id) => id,
		Target::Url(url) => resolve_status(client, access_token, &url)?.poll.context("Post has no poll")?.id,
	};
	client.vote_poll(access_token, &poll_id, choices)
}

/// An account's client and token, for tasks on the UI thread that call the server themselves.
/// Signing in to an account, or switching to it, makes a new one.
#[derive(Clone)]
pub struct Session {
	id: u64,
	client: MastodonClient,
	token: String,
	writes: Queue,
}

impl Session {
	pub fn new(client: MastodonClient, token: String) -> Self {
		static NEXT_ID: AtomicU64 = AtomicU64::new(0);
		Self { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), client, token, writes: Queue::new() }
	}

	pub const fn is(&self, other: &Self) -> bool {
		self.id == other.id
	}

	/// Starts `request` on a background thread, since the client blocks. Requests made this way
	/// run side by side, so this is for reading.
	pub fn call<T, F>(&self, request: F) -> Background<T>
	where
		F: FnOnce(&MastodonClient, &str) -> T + Send + 'static,
		T: Send + 'static,
	{
		let (client, token) = (self.client.clone(), self.token.clone());
		background_blocking(move || request(&client, &token))
	}

	/// Queues `request` behind the account's other queued requests, so they reach the server in the order
	/// the user made them, such as a favorite and the unfavorite after it.
	pub fn queued<T, F>(&self, request: F) -> Background<T>
	where
		F: FnOnce(&MastodonClient, &str) -> T + Send + 'static,
		T: Send + 'static,
	{
		let (client, token) = (self.client.clone(), self.token.clone());
		self.writes.run(move || request(&client, &token))
	}
}

/// A page of `timeline_type`, the newest unless `max_id` asks for the posts older than it.
pub fn fetch_timeline(
	client: &MastodonClient,
	access_token: &str,
	timeline_type: &TimelineType,
	limit: Option<u32>,
	max_id: Option<&str>,
) -> Result<TimelineData> {
	let mut result = match timeline_type {
		TimelineType::Notifications | TimelineType::Mentions => client
			.get_notifications(access_token, timeline_type, limit, max_id)
			.map(|(n, next)| TimelineData::Notifications(n, next)),
		TimelineType::Direct => {
			client.get_conversations(access_token, limit, max_id).map(|(c, next)| TimelineData::Conversations(c, next))
		}
		TimelineType::Thread { id, .. } => client.get_status(access_token, id).and_then(|focus| {
			client.get_status_context(access_token, id).map(|context| prepare_thread_timeline(focus, context))
		}),
		_ => {
			let mut statuses = Vec::new();
			if let TimelineType::User { id, .. } = timeline_type
				&& max_id.is_none()
				&& let Ok(mut pinned) = client.get_pinned_statuses(access_token, id)
			{
				for p in &mut pinned {
					p.pinned = true;
				}
				statuses.extend(pinned);
			}
			match client.get_timeline(access_token, timeline_type, limit, max_id) {
				Ok((page, next)) => {
					let mut seen: HashSet<String> = statuses.iter().map(|st| st.id.clone()).collect();
					statuses.extend(page.into_iter().filter(|st| seen.insert(st.id.clone())));
					Ok(TimelineData::Statuses(statuses, next))
				}
				// The pinned posts still show when the rest fail.
				Err(e) if statuses.is_empty() => Err(e),
				Err(_) => Ok(TimelineData::Statuses(statuses, None)),
			}
		}
	};
	// Remote instances fetch anonymously, so their statuses never carry our filter matches.
	if let TimelineType::InstanceLocal { .. } = timeline_type
		&& let Ok(TimelineData::Statuses(statuses, _)) = &mut result
		&& let Ok(filters) = client.get_filters(access_token)
	{
		for status in statuses {
			status.apply_filters(&filters);
		}
	}
	result
}

/// The thread around `focus`, which is already known, so only its context is fetched.
pub fn fetch_thread(client: &MastodonClient, access_token: &str, focus: Status) -> Result<TimelineData> {
	client.get_status_context(access_token, &focus.id).map(|context| prepare_thread_timeline(focus, context))
}

fn prepare_thread_timeline(focus: Status, context: StatusContext) -> TimelineData {
	let mut statuses = context.ancestors;
	statuses.push(focus);
	statuses.extend(context.descendants);
	let mut status_map: HashMap<String, Status> = HashMap::new();
	let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
	for status in statuses {
		status_map.insert(status.id.clone(), status.clone());
		if let Some(parent_id) = &status.in_reply_to_id {
			children_map.entry(parent_id.clone()).or_default().push(status.id.clone());
		}
	}
	let mut roots: Vec<String> = status_map
		.values()
		.filter(|s| s.in_reply_to_id.as_ref().is_none_or(|parent_id| !status_map.contains_key(parent_id)))
		.map(|s| s.id.clone())
		.collect();
	let sort_key = |s: &Status| {
		let time: Option<DateTime<chrono::Utc>> = s.created_at.parse().ok();
		(time, s.id.clone())
	};
	roots.sort_by(|a_id, b_id| {
		let a = &status_map[a_id];
		let b = &status_map[b_id];
		sort_key(a).cmp(&sort_key(b))
	});
	let mut sorted_statuses: Vec<Status> = Vec::new();
	let mut visited: HashSet<String> = HashSet::new();
	let mut stack: Vec<String> = roots;
	stack.reverse();
	while let Some(current_id) = stack.pop() {
		if !visited.insert(current_id.clone()) {
			continue;
		}
		if let Some(status) = status_map.get(&current_id) {
			sorted_statuses.push(status.clone());
			if let Some(children) = children_map.get_mut(&current_id) {
				children.sort_by(|a_id, b_id| {
					let a = &status_map[a_id];
					let b = &status_map[b_id];
					sort_key(a).cmp(&sort_key(b))
				});
				for child_id in children.iter().rev() {
					stack.push(child_id.clone());
				}
			}
		}
	}
	sorted_statuses.reverse();
	TimelineData::Statuses(sorted_statuses, None)
}

fn first_relationship_result(relationships: Vec<Relationship>) -> Result<Relationship> {
	relationships.into_iter().next().ok_or_else(|| anyhow::anyhow!("Relationship lookup returned no results"))
}

/// Finds the post at `url` on the account's own instance, which a post from elsewhere must be
/// before the account can act on it.
pub fn resolve_status(client: &MastodonClient, access_token: &str, url: &str) -> Result<Status> {
	client
		.search(access_token, url, SearchType::Statuses, Some(1), None)
		.and_then(|mut res| res.statuses.pop().context("Status not found on local instance"))
}

/// How long a mute lasts, and whether it hides the account's notifications as well as its posts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MuteOptions {
	/// Seconds, with 0 for no end.
	pub duration: u32,
	pub notifications: bool,
}

/// The lengths Mastodon's own web app offers, with 0 for no end.
pub const MUTE_DURATIONS: [(u32, &str); 8] = [
	(0, "Indefinitely"),
	(5 * 60, "5 minutes"),
	(30 * 60, "30 minutes"),
	(60 * 60, "1 hour"),
	(6 * 60 * 60, "6 hours"),
	(24 * 60 * 60, "1 day"),
	(3 * 24 * 60 * 60, "3 days"),
	(7 * 24 * 60 * 60, "7 days"),
];

/// Whether a followed account's boosts and notifications come through.
#[derive(Debug, Clone, Copy)]
pub struct FollowOptions {
	pub reblogs: bool,
	pub notify: bool,
}

/// A plain follow's: boosts come through, notifications don't.
impl Default for FollowOptions {
	fn default() -> Self {
		Self { reblogs: true, notify: false }
	}
}

/// Changes the relationship with `account_id` as `action` says. The follow actions also set the
/// account's `options`, which the others leave as they are.
pub fn change_relationship(
	client: &MastodonClient,
	access_token: &str,
	account_id: &str,
	action: RelationshipAction,
	options: FollowOptions,
) -> Result<Relationship> {
	match action {
		RelationshipAction::Follow
		| RelationshipAction::ShowBoosts
		| RelationshipAction::HideBoosts
		| RelationshipAction::EnableNotifications
		| RelationshipAction::DisableNotifications => {
			client.follow_account_with_options(access_token, account_id, options.reblogs, options.notify)
		}
		RelationshipAction::Unfollow | RelationshipAction::CancelFollowRequest => {
			client.unfollow_account(access_token, account_id)
		}
		RelationshipAction::AcceptFollowRequest => client.authorize_follow_request(access_token, account_id),
		RelationshipAction::RejectFollowRequest => client.reject_follow_request(access_token, account_id),
		RelationshipAction::Block => client.block_account(access_token, account_id),
		RelationshipAction::Unblock => client.unblock_account(access_token, account_id),
		RelationshipAction::Mute(mute) => {
			client.mute_account(access_token, account_id, mute.notifications, mute.duration)
		}
		RelationshipAction::Unmute => client.unmute_account(access_token, account_id),
		RelationshipAction::ToggleFollow => toggle_follow(client, access_token, Some(account_id.to_owned()), "").1,
	}
}

/// Follows the account, or unfollows it or cancels the request if it's already followed or asked
/// for. `account_id` is looked up from `acct` when it isn't known. Gives back what was done.
pub fn toggle_follow(
	client: &MastodonClient,
	access_token: &str,
	account_id: Option<String>,
	acct: &str,
) -> (RelationshipAction, Result<Relationship>) {
	let current = account_id
		.map_or_else(|| client.lookup_account(access_token, acct).map(|account| account.id), Ok)
		.and_then(|id| Ok((fetch_relationship(client, access_token, &id)?, id)));
	match current {
		Ok((rel, id)) if rel.following => (RelationshipAction::Unfollow, client.unfollow_account(access_token, &id)),
		Ok((rel, id)) if rel.requested => {
			(RelationshipAction::CancelFollowRequest, client.unfollow_account(access_token, &id))
		}
		Ok((_, id)) => (RelationshipAction::Follow, client.follow_account_with_options(access_token, &id, true, false)),
		Err(err) => (RelationshipAction::ToggleFollow, Err(err)),
	}
}

pub fn fetch_relationship(client: &MastodonClient, access_token: &str, account_id: &str) -> Result<Relationship> {
	client.get_relationships(access_token, slice::from_ref(&account_id.to_owned())).and_then(first_relationship_result)
}

/// The tags named, skipping any that can't be fetched, each marked with whether Fedra's filter
/// mutes it.
pub fn fetch_tags(client: &MastodonClient, access_token: &str, names: &[String]) -> Vec<Tag> {
	let mut tags: Vec<Tag> = names.iter().filter_map(|name| client.get_tag(access_token, name).ok()).collect();
	mark_muted_tags(client, access_token, &mut tags);
	tags
}

/// The hashtags the user follows, sorted by name, each marked as muted or not.
pub fn fetch_followed_tags(client: &MastodonClient, access_token: &str) -> Result<Vec<Tag>> {
	let mut tags = client.get_followed_tags(access_token)?;
	tags.sort_by_cached_key(|tag| tag.name.to_lowercase());
	mark_muted_tags(client, access_token, &mut tags);
	Ok(tags)
}

/// Marks which of `tags` Fedra's own mute filter holds. A failed lookup leaves them all unmuted.
fn mark_muted_tags(client: &MastodonClient, access_token: &str, tags: &mut [Tag]) {
	if let Ok(filters) = client.get_filters(access_token)
		&& let Some(filter) = filters.iter().find(|f| is_muted_tags_filter(f))
	{
		for tag in tags {
			let keyword = format!("#{}", tag.name);
			tag.muted = filter.keywords.iter().any(|kw| kw.keyword.eq_ignore_ascii_case(&keyword));
		}
	}
}

pub fn mute_tag(client: &MastodonClient, access_token: &str, name: &str) -> Result<()> {
	let filters = client.get_filters(access_token)?;
	let keyword = format!("#{name}");
	if let Some(filter) = filters.into_iter().find(is_muted_tags_filter) {
		if !filter.keywords.iter().any(|kw| kw.keyword.eq_ignore_ascii_case(&keyword)) {
			client.add_filter_keyword(access_token, &filter.id, &keyword, true)?;
		}
	} else {
		client.create_filter(
			access_token,
			MUTED_TAGS_FILTER,
			&[
				FilterContext::Home,
				FilterContext::Notifications,
				FilterContext::Public,
				FilterContext::Thread,
				FilterContext::Account,
			],
			&FilterAction::Hide,
			&[(keyword, true)],
			None,
		)?;
	}
	Ok(())
}

pub fn unmute_tag(client: &MastodonClient, access_token: &str, name: &str) -> Result<()> {
	let filters = client.get_filters(access_token)?;
	let keyword = format!("#{name}");
	for filter in filters.iter().filter(|f| is_muted_tags_filter(f)) {
		for kw in filter.keywords.iter().filter(|kw| kw.keyword.eq_ignore_ascii_case(&keyword)) {
			client.delete_filter_keyword(access_token, &kw.id)?;
		}
	}
	Ok(())
}
