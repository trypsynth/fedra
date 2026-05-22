# Fedra

Fedra is a lightweight, fast, and accessible Mastodon client for Windows. It is designed to be completely usable with screen readers and keyboard navigation, providing a seamless social media experience without the bloat.

## Documentation

For a comprehensive user guide, including a full list of features and hotkeys, please see the [User Manual](doc/readme.md).

## Building

To build, you'll need cargo, as well as CMake and Ninja for building wxDragon. In addition, you also need LLVM, from LLVM.org.

### Toolchains

- Stable Rust `1.88.0` is used for builds, tests, and clippy.
- Nightly Rust is only required for formatting with `cargo +nightly fmt`.

### Native Dependencies

Fedra expects a local `wxWidgets` checkout and the `WXWIDGETS_DIR` environment variable to point to it.

On PowerShell:

```powershell
git clone --recurse-submodules https://github.com/wxWidgets/wxWidgets.git
$env:WXWIDGETS_DIR = "$PWD\\wxWidgets"
```

```batch
cargo build --release
```

This will generate the executable at `target/release/fedra.exe`.

### Optional Tools

The following tools aren't required to build a functioning Fedra on a basic level, but will help you make a complete release build.

* `pandoc` on your `PATH` to generate the HTML readme.
* InnoSetup installed to create the installer.

## Before Committing

Run the formatter before you commit changes:

```batch
cargo +nightly fmt
```

Set up the repository pre-commit hooks from the repo root:

```batch
cargo install prek
prek install
```

Run the same checks that CI expects:

```batch
cargo +nightly fmt --check
cargo test
cargo clippy --release
```

## License

This project is licensed under the [MIT License](LICENSE).
