# Builds the Rust core for the S24+ (arm64) and regenerates Kotlin bindings. Run from repo root.
$ErrorActionPreference = "Stop"
$env:ANDROID_NDK_HOME = "$env:LOCALAPPDATA\Android\Sdk\ndk\27.0.12077973"
rustup target add aarch64-linux-android
cargo ndk -t arm64-v8a -P 31 -o spikes/android-app/app/src/main/jniLibs build -p bridge-core --release
cargo build -p bridge-core
cargo run -q -p uniffi-bindgen -- generate --library target/debug/bridge_core.dll --language kotlin --out-dir spikes/android-app/app/src/main/java --no-format
