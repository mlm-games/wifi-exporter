use anyhow::Context;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct WifiCred {
    pub ssid: String,
    #[serde(alias = "password")]
    pub pass: Option<String>,
}

fn shell_escape(s: &str) -> String {
    s.replace("'", "'\\''")
}

fn run_su_cmd(cmd: &str) -> anyhow::Result<String> {
    let out = Command::new("su")
        .arg("-c")
        .arg(cmd)
        .output()
        .context("Failed to execute su command")?;

    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

    if out.status.success() {
        if stdout.is_empty() {
            Ok("OK".to_string())
        } else {
            Ok(stdout)
        }
    } else {
        Err(anyhow::anyhow!(
            "{}",
            if stderr.is_empty() { &stdout } else { &stderr }
        ))
    }
}

pub fn su_cat(path: &str) -> anyhow::Result<String> {
    let cmd = format!("cat '{}' 2>/dev/null", path);
    run_su_cmd(&cmd)
}

pub fn get_api_level() -> i32 {
    Command::new("getprop")
        .arg("ro.build.version.sdk")
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<i32>()
                .ok()
        })
        .unwrap_or(26)
}

/// Adds a single network to system (Android 11+ only)
pub fn su_add_network(cred: &WifiCred) -> anyhow::Result<String> {
    let api = get_api_level();
    if api < 30 {
        return Err(anyhow::anyhow!(
            "Requires Android 11+ (current: API {})",
            api
        ));
    }

    let ssid = shell_escape(&cred.ssid);
    let pass = cred.pass.as_deref().unwrap_or("");

    let cmd = if pass.is_empty() {
        format!("cmd wifi add-network '{}' open", ssid)
    } else {
        let esc_pass = shell_escape(pass);
        format!("cmd wifi add-network '{}' wpa2 '{}'", ssid, esc_pass)
    };

    run_su_cmd(&cmd)
}

/// Bulk import all networks to system (Android 11+ only)
pub fn su_import_all(creds: &[WifiCred]) -> (usize, usize, Vec<(String, String)>) {
    let mut success = 0;
    let mut failed = 0;
    let mut errors = Vec::new();

    for cred in creds {
        match su_add_network(cred) {
            Ok(_) => success += 1,
            Err(e) => {
                failed += 1;
                errors.push((cred.ssid.clone(), e.to_string()));
            }
        }
    }

    (success, failed, errors)
}

pub fn parse_imported_json(json: &str) -> anyhow::Result<Vec<WifiCred>> {
    if let Ok(creds) = serde_json::from_str::<Vec<WifiCred>>(json) {
        return Ok(creds);
    }

    #[derive(Deserialize)]
    struct Wrapped {
        networks: Vec<WifiCred>,
    }
    if let Ok(wrapped) = serde_json::from_str::<Wrapped>(json) {
        return Ok(wrapped.networks);
    }

    Err(anyhow::anyhow!("Invalid JSON format"))
}

pub fn build_json(creds: &[WifiCred]) -> String {
    serde_json::to_string_pretty(creds).unwrap_or_else(|_| "[]".to_string())
}

pub fn parse_wifi_configstore_xml(xml: &str) -> Vec<WifiCred> {
    let mut out = Vec::new();
    for blk in xml.split("<Network>").skip(1) {
        let end = blk.find("</Network>").unwrap_or(blk.len());
        let chunk = &blk[..end];
        let ssid = find_string(chunk, "SSID")
            .or_else(|| find_string(chunk, "ConfigKey"))
            .map(strip_quotes);
        let psk = find_string(chunk, "PreSharedKey").map(strip_quotes);
        if let Some(s) = ssid {
            out.push(WifiCred { ssid: s, pass: psk });
        }
    }
    out
}

fn find_string(hay: &str, name: &str) -> Option<String> {
    let needle = format!("<string name=\"{}\">", name);
    let i = hay.find(&needle)?;
    let start = i + needle.len();
    let rest = &hay[start..];
    let end = rest.find("</string>")?;
    Some(html_unescape(rest[..end].trim()))
}

fn html_unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn strip_quotes<S: AsRef<str>>(s: S) -> String {
    let t = s.as_ref().trim();
    if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

pub fn parse_wpa_supplicant(conf: &str) -> Vec<WifiCred> {
    let mut out = Vec::new();
    let mut ssid: Option<String> = None;
    let mut psk: Option<String> = None;
    let mut in_blk = false;

    for line in conf.lines() {
        let l = line.trim();
        if l.starts_with("network={") {
            in_blk = true;
            ssid = None;
            psk = None;
            continue;
        }
        if in_blk && l.starts_with('}') {
            if let Some(s) = ssid.take() {
                out.push(WifiCred {
                    ssid: s,
                    pass: psk.take(),
                });
            }
            in_blk = false;
            continue;
        }
        if !in_blk {
            continue;
        }
        if let Some(eq) = l.find('=') {
            let k = &l[..eq];
            let v = &l[eq + 1..];
            match k {
                "ssid" => ssid = Some(strip_quotes(v)),
                "psk" => psk = Some(strip_quotes(v)),
                _ => {}
            }
        }
    }
    out
}

pub fn try_read_with_su() -> anyhow::Result<Vec<WifiCred>> {
    let candidates = [
        "/data/misc/apexdata/com.android.wifi/WifiConfigStore.xml",
        "/data/misc/wifi/WifiConfigStore.xml",
        "/data/misc/wifi/WifiConfigStore.conf",
        "/data/misc/wifi/wpa_supplicant.conf",
    ];
    for p in candidates {
        if let Ok(text) = su_cat(p) {
            info!("Read config via su: {}", p);
            if p.ends_with(".xml") || text.contains("<Network>") {
                let v = parse_wifi_configstore_xml(&text);
                if !v.is_empty() {
                    return Ok(v);
                }
            } else if text.contains("network={") {
                let v = parse_wpa_supplicant(&text);
                if !v.is_empty() {
                    return Ok(v);
                }
            }
        } else {
            warn!("Could not read {}", p);
        }
    }
    Err(anyhow::anyhow!("No readable Wi‑Fi config via su"))
}
