use std::{
	io::{Read, Write},
	net::TcpListener,
	thread,
	time::{Duration, Instant},
};

use url::Url;

use crate::mastodon::MastodonClient;

const CALLBACK_PATH: &str = "/oauth/callback";
const LISTEN_TIMEOUT: Duration = Duration::from_secs(120);
pub const OOB_REDIRECT_URI: &str = "urn:ietf:wg:oauth:2.0:oob";

#[derive(Debug)]
pub enum AuthError {
	Listen,
	Register,
	AuthorizeUrl,
	Browser,
	Timeout,
	TokenExchange,
}

pub struct OAuthCredentials {
	pub access_token: String,
	pub client_id: String,
	pub client_secret: String,
}

pub fn oauth_with_local_listener(client: &MastodonClient, app_name: &str) -> Result<OAuthCredentials, AuthError> {
	let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| AuthError::Listen)?;
	let addr = listener.local_addr().map_err(|_| AuthError::Listen)?;
	let redirect_uri = format!("http://127.0.0.1:{}{}", addr.port(), CALLBACK_PATH);
	let credentials = client.register_app(app_name, &redirect_uri).map_err(|_| AuthError::Register)?;
	let authorize_url = client.build_authorize_url(&credentials, &redirect_uri).map_err(|_| AuthError::AuthorizeUrl)?;
	if webbrowser::open(authorize_url.as_str()).is_err() {
		return Err(AuthError::Browser);
	}
	let code = wait_for_code(listener, addr.port()).ok_or(AuthError::Timeout)?;
	let access_token =
		client.exchange_token(&credentials, &code, &redirect_uri).map_err(|_| AuthError::TokenExchange)?;
	Ok(OAuthCredentials { access_token, client_id: credentials.client_id, client_secret: credentials.client_secret })
}

fn wait_for_code(listener: TcpListener, port: u16) -> Option<String> {
	let _ = listener.set_nonblocking(true);
	let start = Instant::now();
	loop {
		match listener.accept() {
			Ok((mut stream, _)) => {
				let mut buffer = [0u8; 4096];
				let size = match stream.read(&mut buffer) {
					Ok(size) => size,
					Err(_) => return None,
				};
				let request = String::from_utf8_lossy(&buffer[..size]);
				let code = extract_code(&request, port);
				let _ = respond_ok(&mut stream);
				return code;
			}
			Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
				if start.elapsed() > LISTEN_TIMEOUT {
					return None;
				}
				thread::sleep(Duration::from_millis(50));
			}
			Err(_) => return None,
		}
	}
}

fn extract_code(request: &str, port: u16) -> Option<String> {
	let line = request.lines().next()?;
	let line = line.strip_prefix("GET ")?;
	let path_end = line.find(' ')?;
	let path = &line[..path_end];
	if !path.starts_with(CALLBACK_PATH) {
		return None;
	}
	let full = format!("http://127.0.0.1:{}{}", port, path);
	let url = Url::parse(&full).ok()?;
	for (key, value) in url.query_pairs() {
		if key == "code" {
			return Some(value.to_string());
		}
	}
	None
}

fn respond_ok(stream: &mut impl Write) -> std::io::Result<()> {
	let body = "<html><body><h2>Fedra</h2><p>You can return to the app now.</p></body></html>";
	let response =
		format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}", body.len(), body);
	stream.write_all(response.as_bytes())
}
