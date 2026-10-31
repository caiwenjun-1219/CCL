package com.caiwenjun.ccl

import android.app.Activity
import android.util.Log
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.ViewGroup
import android.widget.FrameLayout
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * 游戏画面用的 SurfaceView（M3 里程碑）
 *
 * ## 为什么需要它
 *
 * `libpojavexec` 的 `pojavInit` 第一件事就是
 * `ANativeWindow_acquire(窗口指针)`，窗口指针为 NULL 时是**原生层空指针**，
 * 直接把进程带走（不是可以捕获的异常）。这个指针只有一个来源：
 * `ANativeWindow_fromSurface(env, surface)`。
 *
 * 而 `Surface` 是 ART 的对象，进不了我们在同一进程里建的 HotSpot JVM ——
 * 两个 VM 的对象互不通用。所以桥接只能发生在 ART 侧：
 * 本类拿到 Surface，Rust 侧用 ART 的 JNIEnv 调
 * `Java_net_kdt_pojavlaunch_utils_JREUtils_setupBridgeWindow`（见 `java/surface.rs`）。
 *
 * ## 视图放在哪
 *
 * 挂在 content view 的**最底层**（`addView(view, 0, ...)`），默认被 WebView 盖住。
 * M3 只要求 Surface 真实存在并能建出 EGL surface，不要求可见；等 M4 真正出画面时
 * 再把它翻到上层、同时隐藏 WebView。
 *
 * ## 尺寸
 *
 * 铺满全屏。`pojavInit` 会读 `ANativeWindow_getWidth/Height` 并据此
 * `updateMonitorSize`，所以 Surface 的尺寸就是 GLFW 认为的「显示器尺寸」。
 * MC 全屏时正好用得上；窗口模式的分辨率由 LWJGL 自己缩放。
 */
object GameSurface {
    private const val TAG = "CCLGameSurface"

    @Volatile
    private var host: SurfaceView? = null

    @Volatile
    private var surface: Surface? = null

    @Volatile
    private var surfaceW = 0

    @Volatile
    private var surfaceH = 0

    /** surfaceCreated 只发一次，用闩锁让 Rust 侧能阻塞等待而不是轮询 */
    private val ready = CountDownLatch(1)

    /** 在 `MainActivity.onCreate` 里调用（必须在 super.onCreate 之后） */
    @JvmStatic
    fun attach(activity: Activity) {
        if (host != null) {
            ensureAttached(activity)
            return
        }

        val view = SurfaceView(activity)
        view.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                surface = holder.surface
                surfaceW = view.width
                surfaceH = view.height
                Log.i(TAG, "surfaceCreated ${surfaceW}x${surfaceH}")
                ready.countDown()
            }

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, w: Int, h: Int) {
                // 旋转 / 分屏会走这里，Surface 对象本身可能没变
                surface = holder.surface
                surfaceW = w
                surfaceH = h
                Log.i(TAG, "surfaceChanged ${w}x${h} format=$format")
                ready.countDown()
            }

            override fun surfaceDestroyed(holder: SurfaceHolder) {
                surface = null
                Log.i(TAG, "surfaceDestroyed")
            }
        })

        host = view
        addToWindow(activity, view)
    }

    /**
     * 把 SurfaceView 挂到 **DecorView** 而不是 `android.R.id.content`。
     *
     * `content` 里放的是 WebView，而 Tauri 的 WebView 是在 Rust 侧异步建好之后
     * 才 `setContentView` 的 —— `PhoneWindow.setContentView` 会
     * `mContentParent.removeAllViews()`，挂在 content 里的东西会被整个丢掉
     * （实测：surfaceCreated 之后 1.4 秒就 surfaceDestroyed）。
     *
     * DecorView 是 Activity 窗口的根 FrameLayout，`setContentView` 碰不到它，
     * 所以挂这里最稳。插到 index 0 = 压在内容下面，不干扰 WebView。
     */
    private fun addToWindow(activity: Activity, view: SurfaceView) {
        val decor = activity.window.decorView
        if (decor !is ViewGroup) {
            Log.w(TAG, "decorView 不是 ViewGroup，SurfaceView 挂不上去")
            return
        }
        if (view.parent != null) {
            (view.parent as? ViewGroup)?.removeView(view)
        }
        decor.addView(
            view,
            0,
            FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            )
        )
        Log.i(TAG, "SurfaceView 已挂到 decorView（位于内容下层）")
    }

    /**
     * 确认还挂在窗口上。
     *
     * 兜底用：任何原因导致视图被移除（Activity 重建、某些 ROM 的窗口重组），
     * 下次 `onResume` 会重新挂回去。重复挂不会出错（先移除再挂）。
     */
    @JvmStatic
    fun ensureAttached(activity: Activity) {
        val view = host ?: return
        if (view.parent == null) {
            Log.i(TAG, "SurfaceView 掉出窗口了，重新挂载")
            addToWindow(activity, view)
        }
    }

    /**
     * 等 Surface 就绪。
     *
     * 从 Rust 的普通工作线程调用，最长阻塞 [timeoutMs] 毫秒；超时返回 null。
     * 不返回 `Unit` 也不抛异常，调用方（Rust）只看返回值是否存在。
     */
    @JvmStatic
    fun awaitSurface(timeoutMs: Long): Surface? {
        val current = surface
        if (current != null) {
            return current
        }
        return try {
            if (ready.await(timeoutMs, TimeUnit.MILLISECONDS)) surface else null
        } catch (e: InterruptedException) {
            Thread.currentThread().interrupt()
            null
        }
    }

    @JvmStatic
    fun currentSurface(): Surface? = surface

    @JvmStatic
    fun surfaceWidth(): Int = surfaceW

    @JvmStatic
    fun surfaceHeight(): Int = surfaceH

    /** M4 用：把画面翻到最上层 / 收回底层 */
    @JvmStatic
    fun setOnTop(onTop: Boolean) {
        val v = host ?: return
        v.post {
            v.bringToFront()
            v.setZOrderMediaOverlay(onTop)
        }
    }
}
