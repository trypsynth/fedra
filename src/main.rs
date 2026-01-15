#![windows_subsystem = "windows"]

mod config;

use wxdragon::prelude::*;

fn prompt_for_instance(frame: &Frame) -> Option<String> {
	let dialog = TextEntryDialog::builder(frame, "Enter your Mastodon instance", "Add Account")
		.with_style(TextEntryDialogStyle::Default | TextEntryDialogStyle::ProcessEnter)
		.build();
	let result = dialog.show_modal();
	if result != ID_OK {
		dialog.destroy();
		return None;
	}
	let value = dialog.get_value().unwrap_or_default();
	dialog.destroy();
	let instance = value.trim().to_string();
	return Some(instance);
}

fn main() {
	let _ = wxdragon::main(|_| {
		let frame = Frame::builder().with_title("Fedra").with_size(Size::new(800, 600)).build();
		wxdragon::app::set_top_window(&frame);
		let panel = Panel::builder(&frame).build();
		let sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let timelines_label = StaticText::builder(&panel).with_label("Timelines").build();
		let timelines = ListBox::builder(&panel)
			.with_choices(vec!["Home".to_string(), "Local".to_string(), "Federated".to_string()])
			.build();
		timelines.set_selection(0, true);
		let timeline_content = ListBox::builder(&panel).build();
		let timelines_sizer = BoxSizer::builder(Orientation::Vertical).build();
		timelines_sizer.add(&timelines_label, 0, SizerFlag::All, 8);
		timelines_sizer.add(
			&timelines,
			1,
			SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
			8,
		);
		sizer.add_sizer(&timelines_sizer, 1, SizerFlag::Expand, 0);
		sizer.add(&timeline_content, 3, SizerFlag::Expand | SizerFlag::All, 8);
		panel.set_sizer(sizer, true);
		let frame_sizer = BoxSizer::builder(Orientation::Vertical).build();
		frame_sizer.add(&panel, 1, SizerFlag::Expand | SizerFlag::All, 0);
		frame.set_sizer(frame_sizer, true);
		let store = config::ConfigStore::new();
		let mut config = store.load();
		if config.accounts.is_empty() {
			let instance = prompt_for_instance(&frame);
			let instance = match instance {
				Some(value) => value,
				None => {
					frame.close(true);
					return;
				}
			};
			config.accounts.push(config::Account::new(instance));
			let _ = store.save(&config);
		}
		frame.show(true);
		frame.centre();
	});
}
