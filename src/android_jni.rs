#![cfg(target_os = "android")]
use anyhow::Context;
use jni::JavaVM;
use jni::objects::{JObject, JString, JValue};
use once_cell::sync::OnceCell;
use std::ffi::c_void;
use winit::platform::android::activity::AndroidApp;

static JAVA_VM: OnceCell<JavaVM> = OnceCell::new();

fn get_env_and_activity(app: &AndroidApp) -> anyhow::Result<(jni::AttachGuard, JObject)> {
    let vm = JAVA_VM.get_or_try_init(|| {
        let vm_ptr = unsafe { app.vm_as_ptr() as *mut c_void };
        unsafe { JavaVM::from_raw(vm_ptr.cast()) }
    })?;

    let mut env = vm.attach_current_thread()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as *mut _) };
    Ok((env, activity))
}

pub fn write_json_via_mediastore(
    app: &AndroidApp,
    name: &str,
    json: &str,
) -> anyhow::Result<Option<String>> {
    let (mut env, activity) = get_env_and_activity(app)?;
    let cls = env.find_class("dev/example/wifi/AndroidBridge")?;
    let jname = env.new_string(name)?;
    let jjson = env.new_string(json)?;
    let uri_js = env
        .call_static_method(
            cls,
            "writeJsonToDownloads",
            "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&activity),
                JValue::Object(&JObject::from(jname)),
                JValue::Object(&JObject::from(jjson)),
            ],
        )?
        .l()?;
    if uri_js.is_null() {
        Ok(None)
    } else {
        let jstr = JString::from(uri_js);
        let s = String::from(env.get_string(&jstr)?);
        Ok(Some(s))
    }
}

pub fn share_text(app: &AndroidApp, title: &str, text: &str) -> anyhow::Result<()> {
    let (mut env, activity) = get_env_and_activity(app)?;
    let cls = env.find_class("dev/example/wifi/AndroidBridge")?;
    let t = env.new_string(title)?;
    let body = env.new_string(text)?;
    let mime = env.new_string("application/json")?;
    env.call_static_method(
        cls,
        "shareText",
        "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        &[
            JValue::Object(&activity),
            JValue::Object(&JObject::from(t)),
            JValue::Object(&JObject::from(body)),
            JValue::Object(&JObject::from(mime)),
        ],
    )?;
    Ok(())
}

pub fn shizuku_ensure_permission(app: &AndroidApp, req_code: i32) -> anyhow::Result<i32> {
    let (mut env, activity) = get_env_and_activity(app)?;
    let cls = env.find_class("dev/example/wifi/AndroidBridge")?;
    let code = env
        .call_static_method(
            cls,
            "ensureShizukuPermission",
            "(Landroid/app/Activity;I)I",
            &[JValue::Object(&activity), JValue::Int(req_code)],
        )?
        .i()?;
    Ok(code)
}

pub fn shizuku_cat(app: &AndroidApp, path: &str) -> anyhow::Result<Option<String>> {
    let (mut env, _activity) = get_env_and_activity(app)?;
    let cls = env.find_class("dev/example/wifi/AndroidBridge")?;
    let jpath = env.new_string(path)?;
    let out = env
        .call_static_method(
            cls,
            "shizukuCat",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&JObject::from(jpath))],
        )?
        .l()?;
    if out.is_null() {
        Ok(None)
    } else {
        let jstr = JString::from(out);
        Ok(Some(String::from(env.get_string(&jstr)?)))
    }
}
