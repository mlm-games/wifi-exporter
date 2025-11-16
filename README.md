# Wi‑Fi Passwords Exporter 

A minimal Android app using the Repose UI stack that:

- Reads saved Wi‑Fi credentials (requires root or Shizuku with root/Sui).
- Exports as JSON and saves to Downloads (scoped storage via MediaStore on API 29+; app-specific external dir on API 26–28).
- Shares JSON via Android Sharesheet.

## Build

1. Install `cargo-apk` (or `cargo-ndk`) and Android NDK/SDK.
2. Run `cargo apk run --release -p wifi_exporter_repose`

## Notes

- **Shizuku**: `android/AndroidManifest.xml` declares `ShizukuProvider` and `android/build.gradle.kts` that adds the Shizuku dependencies. `cargo-apk` merges these into the generated Gradle project so the provider class resolves.
- **MediaStore**: On Android 10+ we insert into `MediaStore.Downloads` with `RELATIVE_PATH="Download/"` and toggle `IS_PENDING` during write.

## Usage

1. Tap "Export (Root/Shizuku)" and grant root in your su/Shizuku manager.
2. Tap "Save JSON to Downloads" to persist the exported JSON file.
3. Tap "Share JSON" to open the system Sharesheet.

## Security

- The app never transmits data off-device; sharing is at your discretion.
- Root/Shizuku access only used to read local config files.
