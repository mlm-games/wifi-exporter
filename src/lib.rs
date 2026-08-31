#![cfg(target_os = "android")]
#![allow(improper_ctypes_definitions)]
mod android_jni;
mod parsers;

use android_jni::{share_text, write_json_via_mediastore};
use log::{LevelFilter, warn};
use parsers::{
    WifiCred, build_json, get_api_level, parse_imported_json, su_add_network, su_cat,
    su_import_all, try_read_with_su,
};
use repose_core::prelude::*;
use repose_core::{Alignment, FontWeight, PaddingValues};
use repose_material::material3::{
    Button, ButtonConfig, Card, CardConfig, ElevatedButton, FilledTonalButton, OutlinedButton,
    Surface, SurfaceConfig,
};
use repose_platform::RenderContext;
use repose_platform::android::run_android_app;
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::*;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use winit::platform::android::activity::AndroidApp;

static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();

const MIN_IMPORT_API: i32 = 30;

fn app(_s: &mut Scheduler, _rc: &RenderContext) -> View {
    let creds = remember(|| signal(Vec::<WifiCred>::new()));
    let status = remember(|| signal(String::from("Ready — tap Load System")));
    let json_buf = remember(|| signal(String::new()));
    let api_level = get_api_level();
    let can_import = api_level >= MIN_IMPORT_API;

    let load_system_action = load_system((*creds).clone(), (*status).clone(), (*json_buf).clone());
    let load_file_action = load_file((*creds).clone(), (*status).clone(), (*json_buf).clone());
    let import_to_system_action = import_to_system((*creds).clone(), (*status).clone());
    let save_action = save_json((*status).clone(), (*json_buf).clone());
    let share_action = share_json((*status).clone(), (*json_buf).clone());

    let api_chip_text = if can_import {
        format!("API {} (Import available)", api_level)
    } else {
        format!("API {} (Import needs Android 11+)", api_level)
    };

    let status_for_list = (*status).clone();
    let th = theme();

    repose_material::material3::MaterialTheme(th, || {
        Box(Modifier::new().fill_max_size().background(th.background)).child(
            Column(Modifier::new().fill_max_size().padding(20.0)).with_children(vec![
                Column(
                    Modifier::new()
                        .fill_max_width()
                        .padding_values(PaddingValues {
                            top: 8.0,
                            left: 4.0,
                            right: 4.0,
                            bottom: 12.0,
                        }),
                )
                .with_children(vec![
                    Text("WiFi Exporter")
                        .size(28.0)
                        .color(th.on_background)
                        .font_weight(FontWeight::BOLD),
                    Space(Modifier::new().height(6.0)),
                    Surface(
                        SurfaceConfig {
                            modifier: Modifier::new().clip_rounded(20.0),
                            color: if can_import {
                                th.primary_container
                            } else {
                                th.error_container
                            },
                            content_color: if can_import {
                                th.on_primary_container
                            } else {
                                th.on_error_container
                            },
                            shape_radius: 20.0,
                            ..Default::default()
                        },
                        || {
                            Box(Modifier::new().padding_values(PaddingValues {
                                left: 12.0,
                                right: 12.0,
                                top: 6.0,
                                bottom: 6.0,
                            }))
                            .child(
                                Text(api_chip_text.clone())
                                    .size(12.0)
                                    .font_weight(FontWeight::MEDIUM),
                            )
                        },
                    ),
                ]),
                Space(Modifier::new().height(16.0)),
                Row(Modifier::new().fill_max_width().gap(12.0)).with_children(vec![
                    Button(
                        Modifier::new().weight(1.0),
                        load_system_action.clone(),
                        ButtonConfig::default(),
                        || Text("Load System").size(14.0),
                    ),
                    FilledTonalButton(
                        Modifier::new().weight(1.0),
                        load_file_action.clone(),
                        ButtonConfig::default(),
                        || Text("Load File").size(14.0),
                    ),
                ]),
                Space(Modifier::new().height(12.0)),
                Row(Modifier::new().fill_max_width().gap(12.0)).with_children(vec![
                    OutlinedButton(
                        Modifier::new().weight(1.0),
                        save_action.clone(),
                        ButtonConfig::default(),
                        || Text("Save JSON").size(14.0),
                    ),
                    OutlinedButton(
                        Modifier::new().weight(1.0),
                        share_action.clone(),
                        ButtonConfig::default(),
                        || Text("Share").size(14.0),
                    ),
                ]),
                Space(Modifier::new().height(14.0)),
                {
                    let mut cfg = ButtonConfig::default();
                    cfg.enabled = can_import;
                    ElevatedButton(
                        Modifier::new().fill_max_width().height(48.0),
                        import_to_system_action.clone(),
                        cfg,
                        || {
                            Text("Import All to System")
                                .size(14.0)
                                .font_weight(FontWeight::MEDIUM)
                        },
                    )
                },
                Space(Modifier::new().height(16.0)),
                Surface(
                    SurfaceConfig {
                        modifier: Modifier::new().fill_max_width().clip_rounded(12.0),
                        color: th.surface_container,
                        content_color: th.on_surface,
                        shape_radius: 12.0,
                        ..Default::default()
                    },
                    || {
                        Box(Modifier::new().padding(12.0))
                            .child(Text(status.get()).size(13.0).color(th.on_surface_variant))
                    },
                ),
                Space(Modifier::new().height(16.0)),
                Box(Modifier::new().weight(1.0).fill_max_width()).child(network_list(
                    creds.get(),
                    status_for_list,
                    can_import,
                )),
            ]),
        )
    })
}

fn load_system(
    creds: Signal<Vec<WifiCred>>,
    status: Signal<String>,
    json_buf: Signal<String>,
) -> impl Fn() + Clone + 'static {
    move || {
        status.set("Reading system config…".into());
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
}

fn load_file(
    creds: Signal<Vec<WifiCred>>,
    status: Signal<String>,
    json_buf: Signal<String>,
) -> impl Fn() + Clone + 'static {
    move || {
        status.set("Reading wifi_import.json…".into());

        let paths = [
            "/sdcard/Download/wifi_import.json",
            "/storage/emulated/0/Download/wifi_import.json",
        ];
        let mut content = None;
        for path in &paths {
            if let Ok(text) = su_cat(path) {
                content = Some(text);
                break;
            }
        }

        match content {
            Some(text) => match parse_imported_json(&text) {
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
            None => {
                status.set("File not found: Download/wifi_import.json".into());
            }
        }
    }
}

fn import_to_system(
    creds: Signal<Vec<WifiCred>>,
    status: Signal<String>,
) -> impl Fn() + Clone + 'static {
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

        status.set(format!("Adding {} networks…", list.len()));

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
}

fn save_json(status: Signal<String>, json_buf: Signal<String>) -> impl Fn() + Clone + 'static {
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
}

fn share_json(status: Signal<String>, json_buf: Signal<String>) -> impl Fn() + Clone + 'static {
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
}

fn network_list(creds: Vec<WifiCred>, status: Signal<String>, can_import: bool) -> View {
    let scroll_state = remember_scroll_state("network_list");
    let th = theme();

    let rows: Vec<View> = if creds.is_empty() {
        vec![
            Box(Modifier::new().fill_max_width().padding(32.0)).child(
                Column(
                    Modifier::new()
                        .fill_max_width()
                        .align_items(AlignItems::CENTER),
                )
                .with_children(vec![
                    Text("No networks loaded")
                        .size(15.0)
                        .color(th.on_surface_variant),
                    Space(Modifier::new().height(6.0)),
                    Text("Tap Load System (root) or Load File")
                        .size(12.0)
                        .color(th.outline),
                ]),
            ),
        ]
    } else {
        creds
            .into_iter()
            .map(|c| {
                let cred = c.clone();
                let status_signal = status.clone();

                let add_action = move || {
                    if !can_import {
                        status_signal.set("Import requires Android 11+".into());
                        return;
                    }
                    status_signal.set(format!("Adding '{}'…", cred.ssid));
                    match su_add_network(&cred) {
                        Ok(_) => status_signal.set(format!("✓ Added '{}'", cred.ssid)),
                        Err(e) => status_signal.set(format!("✗ {}: {}", cred.ssid, e)),
                    }
                };

                let pass_display = c.pass.as_deref().unwrap_or("Open • no password");

                Card(
                    CardConfig {
                        modifier: Modifier::new()
                            .fill_max_width()
                            .padding_values(PaddingValues {
                                left: 2.0,
                                right: 2.0,
                                top: 4.0,
                                bottom: 4.0,
                            }),
                        container_color: th.surface_container_low,
                        content_color: th.on_surface,
                        shape_radius: 16.0,
                        tonal_elevation: 1.0,
                        ..Default::default()
                    },
                    {
                        let c = c.clone();
                        move || {
                            Row(Modifier::new().fill_max_width().padding(14.0).gap(12.0))
                                .with_children(vec![
                                    Box(Modifier::new()
                                        .size(40.0, 40.0)
                                        .background(th.primary_container)
                                        .clip_rounded(20.0))
                                    .child(
                                        Box(Modifier::new()
                                            .fill_max_size()
                                            .content_alignment(Alignment::Center))
                                        .child(Text("≋").size(16.0).color(th.on_primary_container)),
                                    ),
                                    Column(Modifier::new().weight(1.0).gap(2.0)).with_children(
                                        vec![
                                            Text(c.ssid.clone())
                                                .size(15.0)
                                                .color(th.on_surface)
                                                .font_weight(FontWeight::MEDIUM)
                                                .overflow_ellipsize(),
                                            Text(pass_display.to_string())
                                                .size(12.0)
                                                .color(th.on_surface_variant)
                                                .overflow_ellipsize(),
                                        ],
                                    ),
                                    {
                                        let mut cfg = ButtonConfig::default();
                                        cfg.enabled = can_import;
                                        FilledTonalButton(
                                            Modifier::new().size(44.0, 44.0),
                                            add_action.clone(),
                                            cfg,
                                            || Text("+").size(18.0),
                                        )
                                    },
                                ])
                        }
                    },
                )
            })
            .collect()
    };

    ScrollArea(
        Modifier::new().fill_max_size(),
        scroll_state,
        Column(Modifier::new().fill_max_width().gap(4.0)).with_children(rows),
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

    rlobkit_app_events::insets::set_on_insets(Box::new(|insets| {
        let r = repose_core::locals::WindowInsets {
            top: insets.top,
            bottom: insets.bottom,
            left: insets.left,
            right: insets.right,
            ime_bottom: insets.ime_bottom,
        };
        repose_core::locals::set_window_insets_default(r);
    }));

    let _ = run_android_app(
        android_app,
        app as fn(&mut Scheduler, &RenderContext) -> View,
    );
}
