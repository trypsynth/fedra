use minijinja::{Environment, context};

pub const DEFAULT_POST_TEMPLATE: &str = "{{ author }}: {{ content }}{% if media or poll %} - {{ media }}{{ poll }}{% endif %} - {{ relative_time }}, {{ visibility }}{% if reply_count %}, {{ reply_count }}{% endif %}{% if boost_count %}, {{ boost_count }}{% endif %}{% if favorite_count %}, {{ favorite_count }}{% endif %}{% if client %}, via {{ client }}{% endif %}";
pub const DEFAULT_BOOST_TEMPLATE: &str = "{{ booster }} boosted {{ author }}: {{ content }}{% if media or poll %} - {{ media }}{{ poll }}{% endif %}{% if quote_author %} - Quoting {{ quote_author }} ({{ quote_username }}): {{ quote_content }}{% if quote_media or quote_poll %} - {{ quote_media }}{{ quote_poll }}{% endif %}{% endif %} - {{ relative_time }}, {{ visibility }}{% if reply_count %}, {{ reply_count }}{% endif %}{% if boost_count %}, {{ boost_count }}{% endif %}{% if favorite_count %}, {{ favorite_count }}{% endif %}{% if client %}, via {{ client }}{% endif %}";
pub const DEFAULT_QUOTE_TEMPLATE: &str = "{{ author }}: {{ content }}{% if media or poll %} - {{ media }}{{ poll }}{% endif %} - Quoting {{ quote_author }} ({{ quote_username }}): {{ quote_content }}{% if quote_media or quote_poll %} - {{ quote_media }}{{ quote_poll }}{% endif %} - {{ relative_time }}, {{ visibility }}{% if reply_count %}, {{ reply_count }}{% endif %}{% if boost_count %}, {{ boost_count }}{% endif %}{% if favorite_count %}, {{ favorite_count }}{% endif %}{% if client %}, via {{ client }}{% endif %}";
pub const DEFAULT_WINDOW_TITLE_TEMPLATE: &str = "Fedra - {{ account }}";

pub struct WindowTitleTemplateVars {
	pub app: String,
	pub account: String,
	pub timeline: String,
}

pub fn render_window_title(template: &str, vars: &WindowTitleTemplateVars) -> String {
	let env = Environment::new();
	let ctx = context! {
		app => vars.app,
		account => vars.account,
		timeline => vars.timeline,
	};
	env.render_str(template, ctx).unwrap_or_else(|_| format!("{} - {}", vars.app, vars.account))
}

pub struct PostTemplateVars {
	pub author: String,
	pub username: String,
	pub content: String,
	pub content_warning: String,
	pub relative_time: String,
	pub absolute_time: String,
	pub visibility: String,
	pub reply_count: String,
	pub boost_count: String,
	pub favorite_count: String,
	pub client: String,
	pub media: String,
	pub poll: String,
	pub booster: String,
	pub booster_username: String,
	pub quote_author: String,
	pub quote_username: String,
	pub quote_content: String,
	pub quote_media: String,
	pub quote_poll: String,
}

pub fn render_template(template: &str, vars: &PostTemplateVars) -> String {
	let env = Environment::new();
	let ctx = context! {
		author => vars.author,
		username => vars.username,
		content => vars.content,
		content_warning => vars.content_warning,
		relative_time => vars.relative_time,
		absolute_time => vars.absolute_time,
		visibility => vars.visibility,
		reply_count => vars.reply_count,
		boost_count => vars.boost_count,
		favorite_count => vars.favorite_count,
		client => vars.client,
		media => vars.media,
		poll => vars.poll,
		booster => vars.booster,
		booster_username => vars.booster_username,
		quote_author => vars.quote_author,
		quote_username => vars.quote_username,
		quote_content => vars.quote_content,
		quote_media => vars.quote_media,
		quote_poll => vars.quote_poll,
	};
	env.render_str(template, ctx).unwrap_or_else(|_| format!("{}: {}", vars.author, vars.content))
}
