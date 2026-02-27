# Fedra

Fedra is a lightweight, fast, and accessible Mastodon client for Windows. It is designed to be completely usable with screen readers and keyboard navigation, providing a seamless social media experience without the bloat.

## Documentation

For a comprehensive user guide, including a full list of features and hotkeys, please see the [User Manual](doc/readme.md).

## Building

To build, you'll need cargo, as well as CMake and Ninja for building wxDragon.

```batch
cargo build --release
```

This will generate the executable at `target/release/fedra.exe`.

### Optional Tools

The following tools aren't required to build a functioning Fedra on a basic level, but will help you make a complete release build.

* `pandoc` on your `PATH` to generate the HTML readme.
* InnoSetup installed to create the installer.

## License

This project is licensed under the [MIT License](LICENSE).
