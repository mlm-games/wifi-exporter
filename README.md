# Wi‑Fi Passwords Exporter 

A minimal Android app using the Repose UI (for android-only testing) that:

- Reads saved Wi‑Fi credentials (requires root).
- Exports as JSON and saves to Downloads (scoped storage via MediaStore on API 29+; app-specific external dir on API 26–28).
- Shares JSON via Android Sharesheet.
- The app is meant to be an example for Repose UI, so will mostly be single featured (to keep jni calls and breakages to minimum).

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

## Import to Device (Android 11+ only)

1. Copy your exported JSON file to the new device
2. Rename it to `wifi_import.json`
3. Place it in the **Downloads** folder (`/sdcard/Download/`)
4. Open the app and tap **"Load File"**
5. Review the networks in the list
6. Tap **"Import All to System"** to add all networks
   - Or tap **+** next to individual networks

> **Note:** On Android 8-10, import buttons will be grayed out. You can still view and re-export passwords.

### JSON Format

```json
[
  {"ssid": "MyNetwork", "pass": "password123"},
  {"ssid": "OpenNetwork", "pass": null}
]
