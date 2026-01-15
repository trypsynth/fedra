use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
	#[error("HTTP request failed: {0}")]
	Http(#[from] reqwest::Error),
	#[error("Invalid URL: {0}")]
	Url(#[from] url::ParseError),
	#[error("IO error: {0}")]
	Io(#[from] io::Error),
	#[error("JSON error: {0}")]
	Json(#[from] serde_json::Error),
	#[error("Failed to bind local listener for OAuth callback")]
	ListenerBind,
	#[error("Failed to open browser for authorization")]
	BrowserOpen,
	#[error("OAuth callback timed out waiting for authorization")]
	OAuthTimeout,
	#[error("Failed to register application with instance")]
	AppRegistration(#[source] Box<Error>),
	#[error("Failed to build authorization URL")]
	AuthorizeUrl(#[source] Box<Error>),
	#[error("Failed to exchange authorization code for token")]
	TokenExchange(#[source] Box<Error>),
}

impl Error {
	pub fn user_message(&self) -> &str {
		match self {
			Self::Http(_) => "A network error occurred. Please check your connection.",
			Self::Url(_) => "The instance URL is invalid.",
			Self::Io(_) => "A file system error occurred.",
			Self::Json(_) => "Failed to process data from the server.",
			Self::ListenerBind => "Could not start the OAuth callback listener.",
			Self::BrowserOpen => "Could not open your web browser.",
			Self::OAuthTimeout => "Authorization timed out. Please try again.",
			Self::AppRegistration(_) => "Failed to register the app with your instance.",
			Self::AuthorizeUrl(_) => "Failed to build the authorization URL.",
			Self::TokenExchange(_) => "Failed to exchange the authorization code for a token.",
		}
	}

	pub fn app_registration(err: Error) -> Self {
		Self::AppRegistration(Box::new(err))
	}

	pub fn authorize_url(err: Error) -> Self {
		Self::AuthorizeUrl(Box::new(err))
	}

	pub fn token_exchange(err: Error) -> Self {
		Self::TokenExchange(Box::new(err))
	}
}

/// Type alias for Results using our unified Error type.
pub type Result<T> = std::result::Result<T, Error>;

pub trait ResultExt<T> {
	fn context_app_registration(self) -> Result<T>;
	fn context_authorize_url(self) -> Result<T>;
	fn context_token_exchange(self) -> Result<T>;
}

impl<T> ResultExt<T> for Result<T> {
	fn context_app_registration(self) -> Result<T> {
		self.map_err(Error::app_registration)
	}

	fn context_authorize_url(self) -> Result<T> {
		self.map_err(Error::authorize_url)
	}

	fn context_token_exchange(self) -> Result<T> {
		self.map_err(Error::token_exchange)
	}
}
