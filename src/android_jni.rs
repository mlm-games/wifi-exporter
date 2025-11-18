#![cfg(target_os = "android")]

use jni::JavaVM;
use jni::objects::{JObject, JString, JValue};
use once_cell::sync::OnceCell;
use std::ffi::c_void;
use winit::platform::android::activity::AndroidApp;

static JAVA_VM: OnceCell<JavaVM> = OnceCell::new();

fn get_env_and_activity(app: &AndroidApp) -> anyhow::Result<(jni::AttachGuard, JObject)> {
    let vm = JAVA_VM.get_or_try_init(|| {
        let vm_ptr = app.vm_as_ptr() as *mut c_void;
        // Safety: provided by the Android runtime; do not free.
        unsafe { JavaVM::from_raw(vm_ptr.cast()) }
    })?;

    let env = vm.attach_current_thread()?;
    // Safety: pointer is provided by Android; do not free/delete it.
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as *mut _) };
    Ok((env, activity))
}

fn sdk_int(env: &mut jni::AttachGuard) -> anyhow::Result<i32> {
    let v = env
        .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
        .i()?;
    Ok(v)
}

pub fn write_json_via_mediastore(
    app: &AndroidApp,
    name: &str,
    json: &str,
) -> anyhow::Result<Option<String>> {
    let (mut env, activity) = get_env_and_activity(app)?;
    let api = sdk_int(&mut env)?;

    // Prepare Java strings and store as JObject for JValue::Object
    let jname = env.new_string(name)?;
    let jname_obj = JObject::from(jname);

    let jmime = env.new_string("application/json")?;
    let jmime_obj = JObject::from(jmime);

    if api >= 29 {
        // ContentResolver
        let resolver = env
            .call_method(
                &activity,
                "getContentResolver",
                "()Landroid/content/ContentResolver;",
                &[],
            )?
            .l()?;

        // ContentValues
        let cv = env.new_object("android/content/ContentValues", "()V", &[])?;

        // Column name constants
        let col_display_name = env
            .get_static_field(
                "android/provider/MediaStore$MediaColumns",
                "DISPLAY_NAME",
                "Ljava/lang/String;",
            )?
            .l()?;
        let col_mime = env
            .get_static_field(
                "android/provider/MediaStore$MediaColumns",
                "MIME_TYPE",
                "Ljava/lang/String;",
            )?
            .l()?;
        let col_rel_path = env
            .get_static_field(
                "android/provider/MediaStore$MediaColumns",
                "RELATIVE_PATH",
                "Ljava/lang/String;",
            )?
            .l()?;
        let col_is_pending = env
            .get_static_field(
                "android/provider/MediaStore$MediaColumns",
                "IS_PENDING",
                "Ljava/lang/String;",
            )?
            .l()?;

        // RELATIVE_PATH = "Download/"
        let downloads_dir = env
            .get_static_field(
                "android/os/Environment",
                "DIRECTORY_DOWNLOADS",
                "Ljava/lang/String;",
            )?
            .l()?;
        let rel = {
            let dl = JString::from(downloads_dir);
            let mut s = String::from(env.get_string(&dl)?);
            if !s.ends_with('/') {
                s.push('/');
            }
            env.new_string(s)?
        };
        let rel_obj = JObject::from(rel);

        // cv.put(DISPLAY_NAME, name)
        env.call_method(
            &cv,
            "put",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[
                JValue::Object(&col_display_name),
                JValue::Object(&jname_obj),
            ],
        )?;

        // cv.put(MIME_TYPE, "application/json")
        env.call_method(
            &cv,
            "put",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[JValue::Object(&col_mime), JValue::Object(&jmime_obj)],
        )?;

        // cv.put(RELATIVE_PATH, "Download/")
        env.call_method(
            &cv,
            "put",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[JValue::Object(&col_rel_path), JValue::Object(&rel_obj)],
        )?;

        // cv.put(IS_PENDING, Integer.valueOf(1))
        let one = env
            .call_static_method(
                "java/lang/Integer",
                "valueOf",
                "(I)Ljava/lang/Integer;",
                &[JValue::Int(1)],
            )?
            .l()?;
        env.call_method(
            &cv,
            "put",
            "(Ljava/lang/String;Ljava/lang/Integer;)V",
            &[JValue::Object(&col_is_pending), JValue::Object(&one)],
        )?;

        // Insert into MediaStore.Downloads
        let downloads_uri = env
            .get_static_field(
                "android/provider/MediaStore$Downloads",
                "EXTERNAL_CONTENT_URI",
                "Landroid/net/Uri;",
            )?
            .l()?;
        let uri = env
            .call_method(
                &resolver,
                "insert",
                "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;",
                &[JValue::Object(&downloads_uri), JValue::Object(&cv)],
            )?
            .l()?;
        if uri.is_null() {
            return Ok(None);
        }

        // Write via OutputStream
        let mode = env.new_string("w")?;
        let mode_obj = JObject::from(mode);
        let os = env
            .call_method(
                &resolver,
                "openOutputStream",
                "(Landroid/net/Uri;Ljava/lang/String;)Ljava/io/OutputStream;",
                &[JValue::Object(&uri), JValue::Object(&mode_obj)],
            )?
            .l()?;
        if os.is_null() {
            return Ok(None);
        }
        let bytes = env.byte_array_from_slice(json.as_bytes())?;
        let bytes_obj = JObject::from(bytes);
        env.call_method(&os, "write", "([B)V", &[JValue::Object(&bytes_obj)])?;
        let _ = env.call_method(&os, "flush", "()V", &[]);
        env.call_method(&os, "close", "()V", &[])?;

        // Mark IS_PENDING = 0
        let cv2 = env.new_object("android/content/ContentValues", "()V", &[])?;
        let zero = env
            .call_static_method(
                "java/lang/Integer",
                "valueOf",
                "(I)Ljava/lang/Integer;",
                &[JValue::Int(0)],
            )?
            .l()?;
        env.call_method(
            &cv2,
            "put",
            "(Ljava/lang/String;Ljava/lang/Integer;)V",
            &[JValue::Object(&col_is_pending), JValue::Object(&zero)],
        )?;
        let _ = env.call_method(
            &resolver,
            "update",
            "(Landroid/net/Uri;Landroid/content/ContentValues;Ljava/lang/String;[Ljava/lang/String;)I",
            &[
                JValue::Object(&uri),
                JValue::Object(&cv2),
                JValue::Object(&JObject::null()),
                JValue::Object(&JObject::null()),
            ],
        )?;

        // Return content URI string
        let jstr = env
            .call_method(&uri, "toString", "()Ljava/lang/String;", &[])?
            .l()?;
        let out = String::from(env.get_string(&JString::from(jstr))?);
        Ok(Some(out))
    } else {
        // API 26–28: app-specific external Downloads dir
        let dir_const = env
            .get_static_field(
                "android/os/Environment",
                "DIRECTORY_DOWNLOADS",
                "Ljava/lang/String;",
            )?
            .l()?;
        let dir = env
            .call_method(
                &activity,
                "getExternalFilesDir",
                "(Ljava/lang/String;)Ljava/io/File;",
                &[JValue::Object(&dir_const)],
            )?
            .l()?;
        if dir.is_null() {
            return Ok(None);
        }

        // new File(dir, name)
        let file = env.new_object(
            "java/io/File",
            "(Ljava/io/File;Ljava/lang/String;)V",
            &[JValue::Object(&dir), JValue::Object(&jname_obj)],
        )?;
        // FileOutputStream(file)
        let fos = env.new_object(
            "java/io/FileOutputStream",
            "(Ljava/io/File;)V",
            &[JValue::Object(&file)],
        )?;
        let bytes = env.byte_array_from_slice(json.as_bytes())?;
        let bytes_obj = JObject::from(bytes);
        env.call_method(&fos, "write", "([B)V", &[JValue::Object(&bytes_obj)])?;
        let _ = env.call_method(&fos, "flush", "()V", &[]);
        env.call_method(&fos, "close", "()V", &[])?;

        // Return absolute path
        let jpath = env
            .call_method(&file, "getAbsolutePath", "()Ljava/lang/String;", &[])?
            .l()?;
        let out = String::from(env.get_string(&JString::from(jpath))?);
        Ok(Some(out))
    }
}

pub fn share_text(app: &AndroidApp, title: &str, text: &str) -> anyhow::Result<()> {
    let (mut env, activity) = get_env_and_activity(app)?;

    // Intent(ACTION_SEND)
    let action = env
        .get_static_field(
            "android/content/Intent",
            "ACTION_SEND",
            "Ljava/lang/String;",
        )?
        .l()?;
    let intent = env.new_object(
        "android/content/Intent",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&action)],
    )?;

    // setType("application/json")
    let mime = env.new_string("application/json")?;
    let mime_obj = JObject::from(mime);
    let _ = env.call_method(
        &intent,
        "setType",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&mime_obj)],
    )?;

    // putExtra(EXTRA_TEXT, text)
    let extra_text = env
        .get_static_field("android/content/Intent", "EXTRA_TEXT", "Ljava/lang/String;")?
        .l()?;
    let jtext = env.new_string(text)?;
    let jtext_obj = JObject::from(jtext);
    let _ = env.call_method(
        &intent,
        "putExtra",
        "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&extra_text), JValue::Object(&jtext_obj)],
    )?;

    // chooser = Intent.createChooser(intent, title)
    let jtitle = env.new_string(title)?;
    let jtitle_obj = JObject::from(jtitle);
    let chooser = env
        .call_static_method(
            "android/content/Intent",
            "createChooser",
            "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;",
            &[JValue::Object(&intent), JValue::Object(&jtitle_obj)],
        )?
        .l()?;

    // activity.startActivity(chooser)
    env.call_method(
        &activity,
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[JValue::Object(&chooser)],
    )?;

    Ok(())
}
