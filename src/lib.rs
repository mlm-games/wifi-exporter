#![cfg(target_os = "android")]
mod android_jni;
mod parsers;

use android_jni::{share_text, write_json_via_mediastore};
use log::{LevelFilter, warn};
use once_cell::sync::OnceCell;
use parsers::{
    WifiCred, build_json, get_api_level, parse_imported_json, su_add_network, su_cat,
    su_import_all, try_read_with_su,
};
use repose_core::prelude::*;
use repose_platform::android::run_android_app;
use repose_ui::*;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use winit::platform::android::activity::AndroidApp;

static ANDROID_APP: OnceCell<AndroidApp> = OnceCell::new();

const MIN_IMPORT_API: i32 = 30;

fn app(_s: &mut Scheduler) -> View {
    let creds = remember(|| signal(Vec::<WifiCred>::new()));
    let status = remember(|| signal(String::from("Ready")));
    let json_buf = remember(|| signal(String::new()));
    let api_level = get_api_level();
    let can_import = api_level >= MIN_IMPORT_API;

    // Load system WiFi passwords
    let load_system_action = {
        let creds = creds.clone();
        let status = status.clone();
        let json_buf = json_buf.clone();
        move || {
            status.set("Reading system config...".into());
            match try_read_with_su() {
                Ok(mut v) => {
                    v.retain(|c| !c.ssid.is_empty());
                    v.sort_by(|a, b| a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase()));
                    let count = v.len();
                    let json = build_json(&v);
                    creds.set(v);
                    json_buf.set(json);
                    status.set(format!("Loaded {} networks", count));
                }
                Err(e) => {
                    warn!("Load failed: {e:?}");
                    status.set("Failed: root denied or config not found".into());
                }
            }
        }
    };

    let load_file_action = {
        let creds = creds.clone();
        let status = status.clone();
        let json_buf = json_buf.clone();
        move || {
            status.set("Reading wifi_import.json...".into());

            let path = "/sdcard/Download/wifi_import.json";
            match su_cat(path) {
                Ok(content) => match parse_imported_json(&content) {
                    Ok(imported) => {
                        let import_count = imported.len();

                        let mut map: HashMap<String, WifiCred> = HashMap::new();
                        for c in imported {
                            map.insert(c.ssid.to_lowercase(), c);
                        }
                        for c in creds.get() {
                            map.insert(c.ssid.to_lowercase(), c);
                        }

                        let mut merged: Vec<WifiCred> = map.into_values().collect();
                        merged.retain(|c| !c.ssid.is_empty());
                        merged.sort_by(|a, b| a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase()));

                        let total = merged.len();
                        let json = build_json(&merged);
                        creds.set(merged);
                        json_buf.set(json);
                        status.set(format!("Imported {}. Total: {}", import_count, total));
                    }
                    Err(e) => {
                        warn!("Parse error: {e:?}");
                        status.set("Invalid JSON format".into());
                    }
                },
                Err(_) => {
                    status.set("File not found: Download/wifi_import.json".into());
                }
            }
        }
    };

    // Import all to system (Android 11+ only)
    let import_to_system_action = {
        let creds = creds.clone();
        let status = status.clone();
        move || {
            if get_api_level() < MIN_IMPORT_API {
                status.set(format!(
                    "Import requires Android 11+ (API {}+)",
                    MIN_IMPORT_API
                ));
                return;
            }

            let list = creds.get();
            if list.is_empty() {
                status.set("No networks loaded".into());
                return;
            }

            status.set(format!("Adding {} networks...", list.len()));

            let (success, failed, errors) = su_import_all(&list);

            if failed == 0 {
                status.set(format!("✓ Added {} networks", success));
            } else {
                let first_err = errors
                    .first()
                    .map(|(s, e)| format!("{}: {}", s, e))
                    .unwrap_or_default();
                status.set(format!(
                    "Added {}, failed {}. {}",
                    success, failed, first_err
                ));
            }
        }
    };

    let save_action = {
        let status = status.clone();
        let json_buf = json_buf.clone();
        move || {
            let json = json_buf.get();
            if json.is_empty() {
                status.set("Nothing to save".into());
                return;
            }
            let fname = format!("wifi_passwords_{}.json", ts_secs());
            if let Some(app) = ANDROID_APP.get() {
                match write_json_via_mediastore(app, &fname, &json) {
                    Ok(Some(_)) => status.set(format!("Saved: {}", fname)),
                    Ok(None) => status.set("Save failed".into()),
                    Err(e) => {
                        warn!("Save error: {e:?}");
                        status.set("Save failed".into());
                    }
                }
            }
        }
    };

    let share_action = {
        let status = status.clone();
        let json_buf = json_buf.clone();
        move || {
            let json = json_buf.get();
            if json.is_empty() {
                status.set("Nothing to share".into());
                return;
            }
            if let Some(app) = ANDROID_APP.get() {
                if let Err(e) = share_text(app, "WiFi Passwords", &json) {
                    warn!("Share error: {e:?}");
                    status.set("Share failed".into());
                }
            }
        }
    };

    let import_btn_color = if can_import {
        Color::from_hex("#c53f16ff")
    } else {
        Color::from_hex("#555555")
    };

    let api_text = if can_import {
        format!("API {} | Import: ✓", api_level)
    } else {
        format!("API {} | Import: ✗ (needs 11+)", api_level)
    };

    let api_color = if can_import {
        Color::from_hex("#888888")
    } else {
        Color::from_hex("#FF8888")
    };

    let status_for_list = (*status).clone();

    Surface(
        Modifier::new()
            .fill_max_size()
            .background(Color::from_hex("#121212")),
        Column(Modifier::new().fill_max_size().padding(24.0)).with_children(vec![
            Space(Modifier::new().height(16.0)),
            Text("WiFi Passwords").size(22.0).color(Color::WHITE),
            Text(api_text).size(12.0).color(api_color),
            Space(Modifier::new().height(16.0)),
            Row(Modifier::new().fill_max_width()).with_children(vec![
                styled_button(
                    "Load System",
                    Color::from_hex("#2186F3"),
                    load_system_action,
                ),
                Space(Modifier::new().width(8.0)),
                styled_button("Load File", Color::from_hex("#2186F3"), load_file_action),
            ]),
            Space(Modifier::new().height(30.0)),
            Row(Modifier::new().fill_max_width()).with_children(vec![
                styled_button("Save JSON", Color::from_hex("#4CAF50"), save_action),
                Space(Modifier::new().width(8.0)),
                styled_button("Share", Color::from_hex("#4CAF50"), share_action),
            ]),
            Space(Modifier::new().height(30.0)),
            Button(
                Text("Import All to System").size(14.0).color(Color::WHITE),
                import_to_system_action,
            )
            .modifier(
                Modifier::new()
                    .fill_max_width()
                    .padding(12.0)
                    .background(import_btn_color)
                    .clip_rounded(8.0),
            ),
            Space(Modifier::new().height(12.0)),
            // Status
            Text(status.get())
                .size(13.0)
                .color(Color::from_hex("#69F0AE")),
            Space(Modifier::new().height(12.0)),
            network_list(creds.get(), status_for_list, can_import),
        ]),
    )
}

fn styled_button<F: Fn() + Clone + 'static>(label: &str, bg: Color, action: F) -> View {
    Button(Text(label).size(13.0).color(Color::WHITE), action).modifier(
        Modifier::new()
            .weight(1.0)
            .padding(10.0)
            .background(bg)
            .clip_rounded(6.0),
    )
}

fn network_list(creds: Vec<WifiCred>, status: Signal<String>, can_import: bool) -> View {
    use repose_ui::scroll::{ScrollArea, remember_scroll_state};
    let scroll_state = remember_scroll_state("network_list");

    let rows: Vec<View> = if creds.is_empty() {
        vec![
            Text("No networks loaded")
                .size(14.0)
                .color(Color::from_hex("#666666"))
                .modifier(Modifier::new().padding(16.0)),
        ]
    } else {
        creds
            .into_iter()
            .map(|c| {
                let ssid = c.ssid.clone();
                let cred = c.clone();
                let status_signal = status.clone();

                let add_action = move || {
                    if !can_import {
                        status_signal.set("Import requires Android 11+".into());
                        return;
                    }
                    status_signal.set(format!("Adding '{}'...", ssid));
                    match su_add_network(&cred) {
                        Ok(_) => status_signal.set(format!("✓ Added '{}'", ssid)),
                        Err(e) => status_signal.set(format!("✗ {}: {}", ssid, e)),
                    }
                };

                let btn_color = if can_import {
                    Color::from_hex("#4CAF50")
                } else {
                    Color::from_hex("#444444")
                };

                let pass_display = c.pass.as_deref().unwrap_or("<no password>");

                Row(Modifier::new()
                    .fill_max_width()
                    .padding(8.0)
                    .background(Color::from_hex("#1E1E1E"))
                    .clip_rounded(8.0))
                .with_children(vec![
                    Column(Modifier::new().weight(1.0).padding(4.0)).with_children(vec![
                        Text(&c.ssid).size(15.0).color(Color::WHITE),
                        Text(pass_display)
                            .size(12.0)
                            .color(Color::from_hex("#AAAAAA")),
                    ]),
                    Button(Text("+").size(16.0).color(Color::WHITE), add_action).modifier(
                        Modifier::new()
                            .size(40.0, 40.0)
                            .background(btn_color)
                            .clip_rounded(15.0),
                    ),
                    Space(Modifier::new().width(20.0)),
                ])
                .modifier(Modifier::new().padding(2.0))
            })
            .collect()
    };

    ScrollArea(
        Modifier::new().fill_max_size(),
        scroll_state,
        Column(Modifier::new().fill_max_width()).with_children(rows),
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
