#![cfg(target_os = "android")]

use jni::objects::{JObject, JString};
use jni::{Env, JValue, JavaVM, jni_sig, jni_str};
use std::ffi::c_void;
use std::sync::OnceLock;
use winit::platform::android::activity::AndroidApp;

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();

fn with_env_and_activity<F, T>(app: &AndroidApp, f: F) -> anyhow::Result<T>
where
    F: FnOnce(&mut Env<'_>, &JObject<'_>) -> anyhow::Result<T>,
{
    let vm = JAVA_VM.get_or_init(|| {
        let vm_ptr = app.vm_as_ptr() as *mut c_void;
        // Safety: provided by the Android runtime; do not free.
        // from_raw asserts non-null and does not return Result.
        unsafe { JavaVM::from_raw(vm_ptr.cast()) }
    });

    vm.attach_current_thread(|env| {
        // Safety: pointer is provided by Android; do not free/delete it.
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr() as *mut _) };
        f(env, &activity)
    })
}

fn sdk_int(env: &mut Env<'_>) -> anyhow::Result<i32> {
    let v = env
        .get_static_field(
            jni_str!("android/os/Build$VERSION"),
            jni_str!("SDK_INT"),
            jni_sig!("I"),
        )?
        .i()?;
    Ok(v)
}

pub fn write_json_via_mediastore(
    app: &AndroidApp,
    name: &str,
    json: &str,
) -> anyhow::Result<Option<String>> {
    with_env_and_activity(app, |env, activity| {
        env.ensure_local_capacity(64)?;
        let api = sdk_int(env)?;

        // Prepare Java strings and store as JObject for JValue::Object
        let jname = env.new_string(name)?;
        let jname_obj = JObject::from(jname);

        let jmime = env.new_string("application/json")?;
        let jmime_obj = JObject::from(jmime);

        if api >= 29 {
            // ContentResolver
            let resolver = env
                .call_method(
                    activity,
                    jni_str!("getContentResolver"),
                    jni_sig!("()Landroid/content/ContentResolver;"),
                    &[],
                )?
                .l()?;

            // ContentValues
            let cv = env.new_object(
                jni_str!("android/content/ContentValues"),
                jni_sig!("()V"),
                &[],
            )?;

            // Column name constants
            let col_display_name = env
                .get_static_field(
                    jni_str!("android/provider/MediaStore$MediaColumns"),
                    jni_str!("DISPLAY_NAME"),
                    jni_sig!("Ljava/lang/String;"),
                )?
                .l()?;
            let col_mime = env
                .get_static_field(
                    jni_str!("android/provider/MediaStore$MediaColumns"),
                    jni_str!("MIME_TYPE"),
                    jni_sig!("Ljava/lang/String;"),
                )?
                .l()?;
            let col_rel_path = env
                .get_static_field(
                    jni_str!("android/provider/MediaStore$MediaColumns"),
                    jni_str!("RELATIVE_PATH"),
                    jni_sig!("Ljava/lang/String;"),
                )?
                .l()?;
            let col_is_pending = env
                .get_static_field(
                    jni_str!("android/provider/MediaStore$MediaColumns"),
                    jni_str!("IS_PENDING"),
                    jni_sig!("Ljava/lang/String;"),
                )?
                .l()?;

            // RELATIVE_PATH = "Download/"
            let downloads_dir = env
                .get_static_field(
                    jni_str!("android/os/Environment"),
                    jni_str!("DIRECTORY_DOWNLOADS"),
                    jni_sig!("Ljava/lang/String;"),
                )?
                .l()?;
            let rel = {
                let dl = env.cast_local::<JString>(downloads_dir)?;
                let mut s = dl.try_to_string(env)?;
                if !s.ends_with('/') {
                    s.push('/');
                }
                env.new_string(s)?
            };
            let rel_obj = JObject::from(rel);

            // cv.put(DISPLAY_NAME, name)
            env.call_method(
                &cv,
                jni_str!("put"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
                &[
                    JValue::Object(&col_display_name),
                    JValue::Object(&jname_obj),
                ],
            )?;

            // cv.put(MIME_TYPE, "application/json")
            env.call_method(
                &cv,
                jni_str!("put"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
                &[JValue::Object(&col_mime), JValue::Object(&jmime_obj)],
            )?;

            // cv.put(RELATIVE_PATH, "Download/")
            env.call_method(
                &cv,
                jni_str!("put"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
                &[JValue::Object(&col_rel_path), JValue::Object(&rel_obj)],
            )?;

            // cv.put(IS_PENDING, Integer.valueOf(1))
            let one = env
                .call_static_method(
                    jni_str!("java/lang/Integer"),
                    jni_str!("valueOf"),
                    jni_sig!("(I)Ljava/lang/Integer;"),
                    &[JValue::Int(1)],
                )?
                .l()?;
            env.call_method(
                &cv,
                jni_str!("put"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/Integer;)V"),
                &[JValue::Object(&col_is_pending), JValue::Object(&one)],
            )?;

            // Insert into MediaStore.Downloads
            let downloads_uri = env
                .get_static_field(
                    jni_str!("android/provider/MediaStore$Downloads"),
                    jni_str!("EXTERNAL_CONTENT_URI"),
                    jni_sig!("Landroid/net/Uri;"),
                )?
                .l()?;
            let uri = env
                .call_method(
                    &resolver,
                    jni_str!("insert"),
                    jni_sig!("(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;"),
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
                    jni_str!("openOutputStream"),
                    jni_sig!("(Landroid/net/Uri;Ljava/lang/String;)Ljava/io/OutputStream;"),
                    &[JValue::Object(&uri), JValue::Object(&mode_obj)],
                )?
                .l()?;
            if os.is_null() {
                return Ok(None);
            }
            let bytes = env.byte_array_from_slice(json.as_bytes())?;
            let bytes_obj = JObject::from(bytes);
            env.call_method(
                &os,
                jni_str!("write"),
                jni_sig!("([B)V"),
                &[JValue::Object(&bytes_obj)],
            )?;
            let _ = env.call_method(&os, jni_str!("flush"), jni_sig!("()V"), &[]);
            let _ = env.call_method(&os, jni_str!("close"), jni_sig!("()V"), &[]);

            // Mark IS_PENDING = 0
            let cv2 = env.new_object(
                jni_str!("android/content/ContentValues"),
                jni_sig!("()V"),
                &[],
            )?;
            let zero = env
                .call_static_method(
                    jni_str!("java/lang/Integer"),
                    jni_str!("valueOf"),
                    jni_sig!("(I)Ljava/lang/Integer;"),
                    &[JValue::Int(0)],
                )?
                .l()?;
            env.call_method(
                &cv2,
                jni_str!("put"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/Integer;)V"),
                &[JValue::Object(&col_is_pending), JValue::Object(&zero)],
            )?;
            let _ = env.call_method(
                &resolver,
                jni_str!("update"),
                jni_sig!("(Landroid/net/Uri;Landroid/content/ContentValues;Ljava/lang/String;[Ljava/lang/String;)I"),
                &[
                    JValue::Object(&uri),
                    JValue::Object(&cv2),
                    JValue::Object(&JObject::null()),
                    JValue::Object(&JObject::null()),
                ],
            )?;

            // Return content URI string
            let jstr = env
                .call_method(
                    &uri,
                    jni_str!("toString"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            let jstr = env.cast_local::<JString>(jstr)?;
            let out = jstr.try_to_string(env)?;
            Ok(Some(out))
        } else {
            // API 26–28: app-specific external Downloads dir
            let dir_const = env
                .get_static_field(
                    jni_str!("android/os/Environment"),
                    jni_str!("DIRECTORY_DOWNLOADS"),
                    jni_sig!("Ljava/lang/String;"),
                )?
                .l()?;
            let dir = env
                .call_method(
                    activity,
                    jni_str!("getExternalFilesDir"),
                    jni_sig!("(Ljava/lang/String;)Ljava/io/File;"),
                    &[JValue::Object(&dir_const)],
                )?
                .l()?;
            if dir.is_null() {
                return Ok(None);
            }

            // new File(dir, name)
            let file = env.new_object(
                jni_str!("java/io/File"),
                jni_sig!("(Ljava/io/File;Ljava/lang/String;)V"),
                &[JValue::Object(&dir), JValue::Object(&jname_obj)],
            )?;
            // FileOutputStream(file)
            let fos = env.new_object(
                jni_str!("java/io/FileOutputStream"),
                jni_sig!("(Ljava/io/File;)V"),
                &[JValue::Object(&file)],
            )?;
            let bytes = env.byte_array_from_slice(json.as_bytes())?;
            let bytes_obj = JObject::from(bytes);
            env.call_method(
                &fos,
                jni_str!("write"),
                jni_sig!("([B)V"),
                &[JValue::Object(&bytes_obj)],
            )?;
            let _ = env.call_method(&fos, jni_str!("flush"), jni_sig!("()V"), &[]);
            let _ = env.call_method(&fos, jni_str!("close"), jni_sig!("()V"), &[]);

            // Return absolute path
            let jpath = env
                .call_method(
                    &file,
                    jni_str!("getAbsolutePath"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            let jpath = env.cast_local::<JString>(jpath)?;
            let out = jpath.try_to_string(env)?;
            Ok(Some(out))
        }
    })
}

pub fn share_text(app: &AndroidApp, title: &str, text: &str) -> anyhow::Result<()> {
    with_env_and_activity(app, |env, activity| {
        // Intent(ACTION_SEND)
        let action = env
            .get_static_field(
                jni_str!("android/content/Intent"),
                jni_str!("ACTION_SEND"),
                jni_sig!("Ljava/lang/String;"),
            )?
            .l()?;
        let intent = env.new_object(
            jni_str!("android/content/Intent"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(&action)],
        )?;

        // setType("application/json")
        let mime = env.new_string("application/json")?;
        let mime_obj = JObject::from(mime);
        let _ = env.call_method(
            &intent,
            jni_str!("setType"),
            jni_sig!("(Ljava/lang/String;)Landroid/content/Intent;"),
            &[JValue::Object(&mime_obj)],
        )?;

        // putExtra(EXTRA_TEXT, text)
        let extra_text = env
            .get_static_field(
                jni_str!("android/content/Intent"),
                jni_str!("EXTRA_TEXT"),
                jni_sig!("Ljava/lang/String;"),
            )?
            .l()?;
        let jtext = env.new_string(text)?;
        let jtext_obj = JObject::from(jtext);
        let _ = env.call_method(
            &intent,
            jni_str!("putExtra"),
            jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;"),
            &[JValue::Object(&extra_text), JValue::Object(&jtext_obj)],
        )?;

        // chooser = Intent.createChooser(intent, title)
        let jtitle = env.new_string(title)?;
        let jtitle_obj = JObject::from(jtitle);
        let chooser = env
            .call_static_method(
                jni_str!("android/content/Intent"),
                jni_str!("createChooser"),
                jni_sig!(
                    "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;"
                ),
                &[JValue::Object(&intent), JValue::Object(&jtitle_obj)],
            )?
            .l()?;

        // activity.startActivity(chooser)
        env.call_method(
            activity,
            jni_str!("startActivity"),
            jni_sig!("(Landroid/content/Intent;)V"),
            &[JValue::Object(&chooser)],
        )?;

        Ok(())
    })
}
