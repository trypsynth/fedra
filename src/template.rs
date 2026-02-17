use minijinja::{Environment, context};

pub const DEFAULT_POST_TEMPLATE: &str = "{{ author }}: {{ content }}{{ media }}{{ poll }} - {{ relative_time }}, {{ visibility }}, {{ reply_count }}, {{ boost_count }}, {{ favorite_count }}{% if client %}, via {{ client }}{% endif %}";

pub const DEFAULT_BOOST_TEMPLATE: &str = "{{ booster }} boosted {{ author }}: {{ content }}{{ media }}{{ poll }} - {{ relative_time }}, {{ visibility }}, {{ reply_count }}, {{ boost_count }}, {{ favorite_count }}{% if client %}, via {{ client }}{% endif %}";

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
	};
	env.render_str(template, ctx).unwrap_or_else(|_| format!("{}: {}", vars.author, vars.content))
}
