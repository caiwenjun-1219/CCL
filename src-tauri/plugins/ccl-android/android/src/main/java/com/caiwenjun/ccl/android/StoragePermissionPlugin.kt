package com.caiwenjun.ccl.android

import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Environment
import android.provider.Settings
import androidx.core.content.ContextCompat
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

/**
 * 存储权限插件。
 *
 * 参考 FCL 首启流程：Android 11+ 需要「所有文件访问」权限（MANAGE_EXTERNAL_STORAGE）
 * 才能把 Java 运行时与游戏本体下载到共享外部存储，方便用户直接在文件管理器里管理。
 *
 * - checkStoragePermission：检测当前是否已授予。
 * - requestStoragePermission：跳转系统设置页申请；用户返回后回传结果。
 */
@TauriPlugin
class StoragePermissionPlugin(private val activity: Activity) : Plugin(activity) {

    @Command
    fun checkStoragePermission(invoke: Invoke) {
        val obj = JSObject()
        obj.put("granted", hasPermission())
        invoke.resolve(obj)
    }

    @Command
    fun requestStoragePermission(invoke: Invoke) {
        if (hasPermission()) {
            val obj = JSObject()
            obj.put("granted", true)
            invoke.resolve(obj)
            return
        }
        val intent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION).apply {
                data = android.net.Uri.parse("package:${activity.packageName}")
            }
        } else {
            // Android 10 及以下：跳转到应用详情页，用户手动开启存储权限
            Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                data = android.net.Uri.parse("package:${activity.packageName}")
            }
        }
        startActivityForResult(invoke, intent, "onPermissionResult")
    }

    @ActivityCallback
    fun onPermissionResult(invoke: Invoke, result: ActivityResult) {
        val obj = JSObject()
        obj.put("granted", hasPermission())
        invoke.resolve(obj)
    }

    private fun hasPermission(): Boolean {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Environment.isExternalStorageManager()
        } else {
            ContextCompat.checkSelfPermission(
                activity, android.Manifest.permission.READ_EXTERNAL_STORAGE
            ) == PackageManager.PERMISSION_GRANTED &&
                ContextCompat.checkSelfPermission(
                    activity, android.Manifest.permission.WRITE_EXTERNAL_STORAGE
                ) == PackageManager.PERMISSION_GRANTED
        }
    }
}
