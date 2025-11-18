#![cfg(target_os = "android")]
mod android_jni;
mod parsers;

use android_jni::{share_text, write_json_via_mediastore};
use log::{LevelFilter, info, warn};
use once_cell::sync::OnceCell;
use parsers::{
    WifiCred, build_json, parse_wifi_configstore_xml, parse_wpa_supplicant, try_read_with_su,
};
use repose_core::prelude::*;
use repose_platform::android::run_android_app;
use repose_ui::*;
use std::time::{SystemTime, UNIX_EPOCH};
use winit::platform::android::activity::AndroidApp;

static ANDROID_APP: OnceCell<AndroidApp> = OnceCell::new();

fn app(_s: &mut Scheduler) -> View {
    let creds = remember(|| signal(Vec::<WifiCred>::new()));
    let status = remember(|| signal(String::from("Idle")));
    let json_buf = remember(|| signal(String::new()));

    let export_action = {
        let creds = creds.clone();
        let status = status.clone();
        let json_buf = json_buf.clone();
        move || {
            status.set("Requesting root via su…".into());

            // Root-only read of Wi‑Fi config files
            let mut list: Option<Vec<WifiCred>> = None;
            match try_read_with_su() {
                Ok(v) => list = Some(v),
                Err(e) => {
                    warn!("su export failed: {e:?}");
                    status.set("Failed: root not granted or store not found.".into());
                }
            }

            if let Some(mut v) = list {
                v.retain(|c| !c.ssid.is_empty());
                v.sort_by(|a, b| a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase()));
                let json = build_json(&v);
                info!("WIFI_EXPORT JSON:\n{}", json);
                creds.set(v);
                json_buf.set(json);
                status.set("Done. You can Save/Share below.".into());
            }
        }
    };

    let save_action = {
        let status = status.clone();
        let json_buf = json_buf.clone();
        move || {
            let json = json_buf.get();
            if json.is_empty() {
                status.set("Nothing to save yet.".into());
                return;
            }
            let fname = format!("wifi_passwords_{}.json", ts_secs());
            if let Some(app) = ANDROID_APP.get() {
                match write_json_via_mediastore(app, &fname, &json) {
                    Ok(Some(uri)) => status.set(format!("Saved to Downloads: {}", uri)),
                    Ok(None) => status.set("Save failed (MediaStore/App-ext)".into()),
                    Err(e) => {
                        warn!("Save failed: {e:?}");
                        status.set("Save failed; see logcat.".into());
                    }
                }
            } else {
                status.set("App context not ready.".into());
            }
        }
    };

    let share_action = {
        let status = status.clone();
        let json_buf = json_buf.clone();
        move || {
            let json = json_buf.get();
            if json.is_empty() {
                status.set("Nothing to share yet.".into());
                return;
            }
            if let Some(app) = ANDROID_APP.get() {
                if let Err(e) = share_text(app, "Wi‑Fi Passwords (JSON)", &json) {
                    warn!("Share failed: {e:?}");
                    status.set("Share failed; see logcat.".into());
                }
            } else {
                status.set("App context not ready.".into());
            }
        }
    };

    Surface(
        Modifier::new()
            .fill_max_size()
            .background(Color::from_hex("#121212")),
        Column(Modifier::new().fill_max_size().padding(16.0)).child((
            Text("Wi‑Fi Passwords Exporter")
                .size(20.0)
                .color(Color::from_hex("#FFFFFF")),
            Text("Root is required to read saved Wi‑Fi configs.")
                .size(14.0)
                .color(Color::from_hex("#AAAAAA")),
            Box(Modifier::new().size(1.0, 12.0)),
            repose_ui::Grid(
                2,
                Modifier::new().fill_max_width().padding(4.0),
                vec![
                    Button("Export (Root)", export_action.clone()).modifier(
                        Modifier::new()
                            .fill_max_width()
                            .padding(4.0)
                            .clip_rounded(6.0),
                    ),
                    Button("Save to Downloads", save_action.clone()).modifier(
                        Modifier::new()
                            .fill_max_width()
                            .padding(4.0)
                            .clip_rounded(6.0),
                    ),
                    Button("Share JSON", share_action.clone()).modifier(
                        Modifier::new()
                            .fill_max_width()
                            .padding(4.0)
                            .clip_rounded(6.0),
                    ),
                ],
            ),
            Box(Modifier::new().size(1.0, 8.0)),
            Text(status.get())
                .size(14.0)
                .color(Color::from_hex("#69F0AE")),
            Box(Modifier::new().size(1.0, 12.0)),
            ScrollList(creds.get()),
        )),
    )
}

fn ScrollList(creds: Vec<WifiCred>) -> View {
    use repose_ui::scroll::{ScrollArea, remember_scroll_state};
    let st = remember_scroll_state("wifi_list");

    let rows: Vec<View> = if creds.is_empty() {
        vec![
            Text("No data yet. Tap Export.")
                .color(Color::from_hex("#CCCCCC"))
                .modifier(Modifier::new().padding(8.0)),
        ]
    } else {
        creds
            .into_iter()
            .map(|c| {
                Column(
                    Modifier::new()
                        .padding(10.0)
                        .background(Color::from_hex("#1E1E1E"))
                        .clip_rounded(8.0),
                )
                .child((
                    Text(format!("SSID: {}", c.ssid))
                        .size(16.0)
                        .color(Color::WHITE),
                    Text(format!(
                        "Password: {}",
                        c.pass.clone().unwrap_or_else(|| "<none/enterprise>".into())
                    ))
                    .size(14.0)
                    .color(Color::from_hex("#DDDDDD")),
                ))
                .modifier(Modifier::new().padding(4.0))
            })
            .collect()
    };

    ScrollArea(
        Modifier::new().fill_max_size().padding(4.0),
        st,
        Column(Modifier::new().fill_max_size()).with_children(rows),
    )
}

fn ts_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn android_main(android_app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(LevelFilter::Info));
    let _ = ANDROID_APP.set(android_app.clone());
    let _ = run_android_app(android_app, app as fn(&mut Scheduler) -> View);
}
