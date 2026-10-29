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
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

/**
 * 存储权限插件（同时承载内置资源的落地）。
 *
 * 参考 FCL 首启流程：Android 11+ 需要「所有文件访问」权限（MANAGE_EXTERNAL_STORAGE）
 * 才能把 Java 运行时与游戏本体下载到共享外部存储，方便用户直接在文件管理器里管理。
 *
 * - checkStoragePermission：检测当前是否已授予。
 * - requestStoragePermission：跳转系统设置页申请；用户返回后回传结果。
 *
 * 另外两个命令用于「内置 Java 运行时」（对齐 FCL 把 JRE 打进 APK assets 的做法）：
 * APK 里的 assets 无法被 Rust 侧 std::fs 直接打开，必须借 AssetManager 拷到磁盘：
 * - hasBundledAsset：判断 assets 里有没有某个文件。
 * - copyBundledAsset：把 assets 里的文件流式拷到指定绝对路径。
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

    /**
     * 参数：assets 内的相对路径 / 目标绝对路径
     *
     * 必须带 `@InvokeArg`：release 构建开了 R8（optimization.enable），
     * 没有这个注解时 tauri-api 的 `-keep @InvokeArg` 规则不生效，
     * 字段 src/dest 会被重命名成 a/b，Jackson 按名字取不到值 →
     * 参数静默变成空串（曾导致内置 JRE 检测恒为 false，退回联网下载）。
     */
    @InvokeArg
    class CopyAssetArgs {
        var src: String = ""
        var dest: String = ""
    }

    @Command
    fun hasBundledAsset(invoke: Invoke) {
        val args = invoke.parseArgs(CopyAssetArgs::class.java)
        val obj = JSObject()
        obj.put("exists", assetExists(args.src))
        // 便于排查：把实际收到的参数回传，空串说明参数没解析出来
        obj.put("src", args.src)
        invoke.resolve(obj)
    }

    @Command
    fun copyBundledAsset(invoke: Invoke) {
        val args = invoke.parseArgs(CopyAssetArgs::class.java)
        if (args.src.isBlank() || args.dest.isBlank()) {
            // 参数解析失败时不要再静默往下跑，直接报错暴露问题
            invoke.reject("内置资源参数为空（src=\"${args.src}\", dest=\"${args.dest}\"）")
            return
        }
        try {
            val destFile = java.io.File(args.dest)
            destFile.parentFile?.mkdirs()
            var total = 0L
            // 边读边写，避免把 20 MB 的资源整块读进内存（低端机容易 OOM）
            activity.assets.open(args.src).use { input ->
                java.io.FileOutputStream(destFile).use { output ->
                    val buf = ByteArray(1 shl 20)
                    while (true) {
                        val n = input.read(buf)
                        if (n < 0) break
                        output.write(buf, 0, n)
                        total += n
                    }
                    output.flush()
                }
            }
            val obj = JSObject()
            obj.put("bytes", total)
            invoke.resolve(obj)
        } catch (e: Exception) {
            invoke.reject(e.message ?: "复制内置资源失败")
        }
    }

    private fun assetExists(path: String): Boolean {
        return try {
            activity.assets.open(path).use { true }
        } catch (e: Exception) {
            false
        }
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
