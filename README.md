# Fedra

Fedra is a lightweight, fast, and accessible Mastodon client for Windows. It is designed to be completely usable with screen readers and keyboard navigation, providing a seamless social media experience without the bloat.

## Documentation

For a comprehensive user guide, including a full list of features and hotkeys, please see the [User Manual](doc/readme.md).

## Building

To build Fedra, you will need Rust installed, along with a C++ compiler (MSVC), LLVM, and CMake for building the dependencies.

```batch
cargo build --release
```

This will generate the executable at `target/release/fedra.exe`.

## License

This project is licensed under the [MIT License](LICENSE).
