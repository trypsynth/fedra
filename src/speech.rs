use std::sync::Mutex;

use tts::Tts;

static TTS: Mutex<Option<Tts>> = Mutex::new(None);

pub fn init() {
	let mut guard = TTS.lock().unwrap();
	if guard.is_none() {
		if let Ok(tts) = Tts::default() {
			*guard = Some(tts);
		}
	}
}

pub fn speak(text: &str) {
	let mut guard = TTS.lock().unwrap();
	if let Some(tts) = guard.as_mut() {
		let _ = tts.speak(text, false);
	}
}

pub fn speak_async(text: &str) {
	let mut guard = TTS.lock().unwrap();
	if let Some(tts) = guard.as_mut() {
		let _ = tts.speak(text, false);
	}
}
