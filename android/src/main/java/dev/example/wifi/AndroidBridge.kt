package dev.example.wifi

import android.app.Activity
import android.content.ContentResolver
import android.content.ContentValues
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.util.Log
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import rikka.shizuku.Shizuku
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileOutputStream
import java.io.OutputStream
import java.lang.reflect.Method

object AndroidBridge {
  private const val TAG = "WifiExporter"
  private const val SHIZUKU_TIMEOUT_MS = 15_000L

  @JvmStatic
  fun writeJsonToDownloads(activity: Activity, displayName: String, json: String): String? {
    return try {
      if (Build.VERSION.SDK_INT >= 29) {
        val cr: ContentResolver = activity.contentResolver
        val cv = ContentValues().apply {
          put(MediaStore.MediaColumns.DISPLAY_NAME, displayName)
          put(MediaStore.MediaColumns.MIME_TYPE, "application/json")
          put(MediaStore.MediaColumns.RELATIVE_PATH, "Download/")
          put(MediaStore.MediaColumns.IS_PENDING, 1)
        }
        val collection = MediaStore.Downloads.EXTERNAL_CONTENT_URI
        val uri = cr.insert(collection, cv) ?: return null

        cr.openOutputStream(uri, "w")?.use { os ->
          os.write(json.toByteArray(Charsets.UTF_8))
          os.flush()
        }

        val done = ContentValues().apply {
          put(MediaStore.MediaColumns.IS_PENDING, 0)
        }
        cr.update(uri, done, null, null)
        uri.toString()
      } else {
        // API 26-28: app-specific external downloads
        var base = activity.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)
        if (base == null) base = activity.getExternalFilesDir(null)
          if (base == null) return null
            if (!base.exists()) base.mkdirs()
              val f = File(base, displayName)
              FileOutputStream(f, false).use { fos ->
                fos.write(json.toByteArray(Charsets.UTF_8))
                fos.flush()
              }
              f.absolutePath
      }
    } catch (t: Throwable) {
      Log.w(TAG, "MediaStore/app-ext write failed", t)
      null
    }
  }

  @JvmStatic
  fun shareText(activity: Activity, title: String, text: String, mime: String?) {
    val send = Intent(Intent.ACTION_SEND).apply {
      putExtra(Intent.EXTRA_TEXT, text)
      putExtra(Intent.EXTRA_TITLE, title)
      type = mime ?: "application/json"
    }
    val chooser = Intent.createChooser(send, null)
    activity.startActivity(chooser)
  }

  /**
   * Ensure Shizuku permission.
   * Returns:
   *   0 = granted
   *   1 = denied
   *   2 = requested (dialog shown)
   *  -1 = unavailable
   */
  @JvmStatic
  fun ensureShizukuPermission(activity: Activity, requestCode: Int): Int {
    return try {
      // Check if Shizuku is available
      if (!Shizuku.pingBinder()) {
        Log.w(TAG, "Shizuku binder not available")
        return -1
      }

      // Check if already granted
      if (Shizuku.checkSelfPermission() == PackageManager.PERMISSION_GRANTED) {
        return 0
      }

      // Check if we should show rationale (user denied before)
      if (Shizuku.shouldShowRequestPermissionRationale()) {
        return 1
      }

      // Request permission asynchronously
      runBlocking {
        if (requestShizukuPermissionAsync(requestCode)) 0 else 1
      }
    } catch (t: Throwable) {
      Log.w(TAG, "Shizuku permission check failed", t)
      -1
    }
  }

  private suspend fun requestShizukuPermissionAsync(requestCode: Int): Boolean {
    if (Shizuku.checkSelfPermission() == PackageManager.PERMISSION_GRANTED) return true

      val deferred = CompletableDeferred<Boolean>()
      val listener = Shizuku.OnRequestPermissionResultListener { _, grantResult ->
        deferred.complete(grantResult == PackageManager.PERMISSION_GRANTED)
      }

      Shizuku.addRequestPermissionResultListener(listener)
      try {
        Shizuku.requestPermission(requestCode)
        return withTimeout(SHIZUKU_TIMEOUT_MS) {
          deferred.await()
        }
      } catch (e: Exception) {
        Log.w(TAG, "Shizuku permission request failed or timed out", e)
        return false
      } finally {
        Shizuku.removeRequestPermissionResultListener(listener)
      }
  }

  /**
   * Use Shizuku to cat a file.
   * Returns file content or null on failure.
   */
  @JvmStatic
  fun shizukuCat(absPath: String): String? {
    return try {
      if (!Shizuku.pingBinder()) {
        Log.w(TAG, "Shizuku binder not available for cat")
        return null
      }

      if (Shizuku.checkSelfPermission() != PackageManager.PERMISSION_GRANTED) {
        Log.w(TAG, "Shizuku permission not granted for cat")
        return null
      }

      val safe = absPath.replace("'", "'\\''")
      val args = arrayOf("sh", "-c", "cat '$safe' 2>/dev/null")
      val process = shizukuNewProcess(args) ?: return null

      val bos = ByteArrayOutputStream()
      process.inputStream.use { input ->
        val buf = ByteArray(8192)
        var r = input.read(buf)
        while (r != -1) {
          bos.write(buf, 0, r)
          r = input.read(buf)
        }
      }
      process.waitFor()
      bos.toString("UTF-8")
    } catch (t: Throwable) {
      Log.w(TAG, "Shizuku cat failed for $absPath", t)
      null
    }
  }

  @Suppress("UNCHECKED_CAST")
  private fun shizukuNewProcess(
    cmd: Array<String>,
    env: Array<String>? = null,
    dir: String? = null
  ): Process? {
    return try {
      val method: Method = Shizuku::class.java.getDeclaredMethod(
        "newProcess",
        Array<String>::class.java,
        Array<String>::class.java,
        String::class.java
      )
      method.isAccessible = true
      method.invoke(null, cmd, env, dir) as Process
    } catch (e: Exception) {
      Log.w(TAG, "Failed to invoke Shizuku.newProcess", e)
      null
    }
  }
}
