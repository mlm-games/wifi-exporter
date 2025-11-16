use anyhow::Context;
use log::{info, warn};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct WifiCred {
    pub ssid: String,
    pub pass: Option<String>,
}

pub fn su_cat(path: &str) -> anyhow::Result<String> {
    let cmd = format!("cat '{}' 2>/dev/null", path);
    let out = Command::new("su")
        .arg("-c")
        .arg(cmd)
        .output()
        .context("exec su")?;
    if out.status.success() && !out.stdout.is_empty() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(anyhow::anyhow!("su cat failed or empty"))
    }
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
    let val = &rest[..end];
    Some(html_unescape(val.trim()))
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

pub fn build_json(creds: &[WifiCred]) -> String {
    let mut s = String::from("[\n");
    for (i, c) in creds.iter().enumerate() {
        let esc = |t: &str| {
            t.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
        };
        s.push_str(&format!(
            "  {{\"ssid\":\"{}\",\"password\":\"{}\"}}{}",
            esc(&c.ssid),
            esc(c.pass.as_deref().unwrap_or("")),
            if i + 1 == creds.len() { "\n" } else { ",\n" }
        ));
    }
    s.push(']');
    s
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
            info!("Read candidate via su: {}", p);
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
