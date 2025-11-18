# Wi‑Fi Passwords Exporter 

A minimal Android app using the Repose UI (for android-only testing) that:

- Reads saved Wi‑Fi credentials (requires root).
- Exports as JSON and saves to Downloads (scoped storage via MediaStore on API 29+; app-specific external dir on API 26–28).
- Shares JSON via Android Sharesheet.

## Build

1. Install `cargo-apk` (or `cargo-ndk`) and Android NDK/SDK.
2. Run `cargo apk run --release -p wifi_exporter_repose`
3. 
## Usage

1. Tap "Export (Root)" and grant root access in your su manager.
2. Tap "Save to Downloads" to persist the exported JSON file.
3. Tap "Share JSON" to open the system Sharesheet.

## Security

- The app never transmits data off-device; sharing is at your discretion.
- Root access only used to read local config files.
