package com.caiwenjun.ccl

import android.app.Activity
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.MotionEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import org.lwjgl.glfw.CallbackBridge
import kotlin.math.abs
import kotlin.math.hypot

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
 * 默认压在内容下层（`addView(view, 0, ...)`），被 WebView 盖住 —— M3 只需要
 * Surface 真实存在、能建出 EGL surface 即可，不要求可见。
 *
 * M4 要「肉眼可见」，于是 [attach] 多了 `overlay` 参数：为 true 时创建阶段就
 * `setZOrderOnTop(true)`，surface 直接盖在 WebView 之上（游戏全屏形态）。
 *
 * **为什么必须在创建时决定**：`setZOrderOnTop` 改的是 surface 图层。创建后再改，
 * 在部分实现/ROM 上会触发 `updateSurface()` 重建 Surface（`surfaceDestroyed` →
 * `surfaceCreated`，`Surface` 换个新对象），而我们手里的 `ANativeWindow` 绑的是
 * 旧的那块 —— EGL 不会报错，画面直接不更新。所以置顶与否一次性定死。
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

    /** Surface 回调与等待方之间不需要额外的锁：字段都是 @Volatile，等待方按 50ms 轮询 */
    private const val POLL_MS = 50L

    /**
     * 第几代 Surface。
     *
     * 每次 `surfaceCreated` 自增。改 z-order 在某些 ROM（本机 vivo 就是）上会
     * 重建 Surface，等待方靠它判断「有没有换过一块新画布」。
     */
    @Volatile
    private var generation = 0

    /** 当前是不是「盖住 WebView」的置顶模式 */
    @Volatile
    private var overlay = false

    /**
     * 在 `MainActivity.onCreate` 里调用（必须在 super.onCreate 之后）
     *
     * @param overlay true = surface 盖在 WebView 之上（游戏画面全屏）；false = 压在内容下层
     */
    @JvmStatic
    @JvmOverloads
    fun attach(activity: Activity, overlay: Boolean = false) {
        if (host != null) {
            ensureAttached(activity)
            return
        }

        val view = SurfaceView(activity)
        if (overlay) {
            // 必须在 surface 创建之前：之后改会重建 Surface，已桥接的 ANativeWindow 作废
            view.setZOrderOnTop(true)
            view.holder.setFormat(android.graphics.PixelFormat.OPAQUE)
            Log.i(TAG, "SurfaceView 以「置顶」模式创建（会盖住 WebView）")
        }
        this.overlay = overlay

        // M4-b-3：SurfaceView 默认**不吃触摸**（不可点击），置顶后事件会直接
        // 穿透到下面的 WebView，游戏收不到任何输入。必须显式打开。
        view.isClickable = true
        view.isFocusable = true
        view.isFocusableInTouchMode = true
        view.setOnTouchListener { v, ev -> handleTouch(v, ev) }

        view.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                surface = holder.surface
                surfaceW = view.width
                surfaceH = view.height
                generation++
                Log.i(TAG, "surfaceCreated ${surfaceW}x${surfaceH} gen=$generation")
            }

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, w: Int, h: Int) {
                // 旋转 / 分屏会走这里，Surface 对象本身可能没变
                surface = holder.surface
                surfaceW = w
                surfaceH = h
                Log.i(TAG, "surfaceChanged ${w}x${h} format=$format")
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
        val lp = FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.MATCH_PARENT
        )
        if (overlay) {
            // 置顶模式：加在最后，surface 层本来就在 window 之上，view 顺序只影响触摸优先级
            decor.addView(view, lp)
            Log.i(TAG, "SurfaceView 已挂到 decorView（置顶，覆盖 WebView）")
        } else {
            decor.addView(view, 0, lp)
            Log.i(TAG, "SurfaceView 已挂到 decorView（位于内容下层）")
        }
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
     *
     * **不能再用一次性闩锁**：`surfaceDestroyed` 之后那块 Surface 就作废了，
     * 而闩锁已经被前面那次 `surfaceCreated` 消耗掉，后来者会「立刻返回 null」，
     * 现象是超时 5000 ms 的消息 0.4 秒就打出来了（vivo V2313A 上实测）。
     * 改成「按条件等、条件变就唤醒」，销毁后再建也能正常等到。
     */
    @JvmStatic
    fun awaitSurface(timeoutMs: Long): Surface? {
        val deadline = SystemClock.uptimeMillis() + timeoutMs
        while (true) {
            val s = surface
            if (s != null && s.isValid()) {
                return s
            }
            if (SystemClock.uptimeMillis() >= deadline) {
                return null
            }
            try {
                Thread.sleep(POLL_MS)
            } catch (e: InterruptedException) {
                Thread.currentThread().interrupt()
                return null
            }
        }
    }

    /**
     * 切到「游戏置顶」并把一块**可用且稳定**的 Surface 准备好（M4-b-2 真机修复）。
     *
     * ## 为什么单独一个入口
     *
     * `setZOrderOnTop` 在 vivo 这类 ROM 上会重建 Surface：`surfaceDestroyed`
     * 之后隔几秒才来新的 `surfaceCreated`。之前的做法是「置顶 → sleep(400)
     * → attach」，正好骑在销毁窗口上，拿到的是死 Surface（或者 null）。
     *
     * 这里改成事件驱动：
     * - 若 generation 变了（= 真的重建了）→ 等新的一块可用再返回
     * - 若在 [SETTLE_MS] 内一直没重建且 Surface 始终可用 → 说明这个 ROM
     *   是原地改图层（MuMu 就是），用当前这块即可
     *
     * @return `OK|...` 或 `ERR:...`
     */
    @JvmStatic
    fun prepareGameSurface(timeoutMs: Long): String {
        val before = generation
        setOverlay(true)

        val deadline = SystemClock.uptimeMillis() + timeoutMs
        var stableSince = 0L
        while (true) {
            val now = SystemClock.uptimeMillis()
            val s = surface
            val usable = s != null && s.isValid()
            if (usable && generation != before) {
                return "OK|重建 gen=$before→$generation ${surfaceW}x${surfaceH}"
            }
            if (usable) {
                if (stableSince == 0L) {
                    stableSince = now
                } else if (now - stableSince >= SETTLE_MS) {
                    return "OK|原地 gen=$generation ${surfaceW}x${surfaceH}"
                }
            } else {
                stableSince = 0L
            }
            if (now >= deadline) {
                return "ERR:置顶后 ${timeoutMs} ms 内没有可用 Surface（gen $before→$generation，surface=${if (s == null) "null" else "invalid"}）"
            }
            try {
                Thread.sleep(POLL_MS)
            } catch (e: InterruptedException) {
                Thread.currentThread().interrupt()
                return "ERR:等待 Surface 时被中断"
            }
        }
    }

    /** 置顶后「没重建」的判定窗口：这段时间内 Surface 一直可用就认为是一次原地改图层 */
    private const val SETTLE_MS = 600L

    @JvmStatic
    fun currentSurface(): Surface? = surface

    @JvmStatic
    fun surfaceWidth(): Int = surfaceW

    @JvmStatic
    fun surfaceHeight(): Int = surfaceH

    /**
     * 收起 / 重新置顶画面（M4-a 的「演示完把 UI 还给用户」）。
     *
     * **务必在渲染结束后调**：`setZOrderOnTop` 会走 `updateSurface()`，
     * 在 AOSP 实现里是改 SurfaceControl 的 layer（实测不重建 Surface，
     * 日志里没有 surfaceDestroyed/surfaceCreated），但这是实现细节 ——
     * 有些 ROM 会重建，届时桥接给 `libpojavexec` 的 `ANativeWindow` 就成了死指针。
     * 所以别边渲染边切。
     */
    @JvmStatic
    fun setOverlay(onTop: Boolean) {
        val v = host ?: return
        v.post {
            if (v.parent == null) {
                return@post
            }
            overlay = onTop
            v.setZOrderMediaOverlay(false)
            v.setZOrderOnTop(onTop)
            Log.i(TAG, "setOverlay($onTop)")
        }
    }

    @JvmStatic
    fun isOverlay(): Boolean = overlay

    // =======================================================================
    // M4-b-3：触摸 → GLFW 输入
    // =======================================================================
    //
    // 直接把事件发给 **ART 侧**的 `CallbackBridge`（`org.lwjgl.glfw` 包下那个
    // Java 类）。这些 native 是 `libpojavexec` 在 ART 的 `JNI_OnLoad` 里注册的，
    // 内嵌 JVM 那份类上没有绑定（探针实测 `UnsatisfiedLinkError`），所以
    // **不能**绕道 Rust 去调 JVM 侧的类。
    //
    // ## 坐标语义（探针实测，见 docs/android-launch-research.md）
    //
    // fork 版 GLFW 的 `glfwSetCursorPos` **真的写回**（读回 = 写入值）。
    // 而 MC 在抓鼠标（游戏内）时每帧都会把光标摆回屏幕中心，所以：
    //
    //   - 抓鼠标（游戏内）：发「中心 + 本次手指增量」，游戏算出 delta = 增量
    //   - 没抓（菜单里）：发绝对坐标，GUI 的光标在哪就点哪
    //
    // MC 的 `MouseHandler` 在一帧内会把多次 onMove 的 delta **累加**，
    // 帧末才摆回中心，所以「每个事件都发中心+增量」不会丢样。
    //
    // ## 手势
    //
    // | 手势 | 行为 |
    // | --- | --- |
    // | 单指拖动 | 移动指针（游戏内 = 转视角） |
    // | 单指轻点 | 左键点击 |
    // | 单指按住 | 左键按住（挖方块 / 连续攻击） |
    // | 双指轻点 | 右键（放置方块 / 使用） |
    // | 双指上下滑 | 滚轮（切物品栏） |
    // | 三指轻点 | ESC（打开游戏菜单） |

    private const val TOUCH_TAG = "CCLInput"

    /** 判定「点」而不是「拖」的位移阈值（像素） */
    private const val TAP_SLOP = 22f

    /** 点按的最长时长；超过就算长按 */
    private const val TAP_MAX_MS = 300L

    /** 长按多久开始按住左键 */
    private const val LONG_PRESS_MS = 380L

    /** 双指滑动多少像素算一格滚轮 */
    private const val SCROLL_STEP = 70f

    /** 菜单里移动指针的灵敏度（1 = 手指走多远光标走多远） */
    private const val SENS_CURSOR = 1.15f

    /** 游戏内转视角的灵敏度 */
    private const val SENS_LOOK = 1.6f

    private val ui = Handler(Looper.getMainLooper())

    private var downX = 0f
    private var downY = 0f
    private var lastX = 0f
    private var lastY = 0f
    private var downAt = 0L
    private var maxPointers = 0
    private var moved = false
    private var leftHeld = false
    private var multiDownAt = 0L
    private var multiMoved = false
    private var scrollAcc = 0f

    /** 菜单模式下的光标位置；<=0 表示还没初始化（首次触摸时置到屏幕中心） */
    private var cursorX = -1f
    private var cursorY = -1f

    /** 已经发过多少条输入事件（首次若干条打日志，方便排查） */
    private var sent = 0

    /** 收到过多少次按下（只日志前几次：确认触摸真的进到 View 层） */
    private var downs = 0

    private val longPress = Runnable {
        if (!moved && !leftHeld && maxPointers == 1) {
            sendButton(0, true)
            leftHeld = true
            Log.i(TOUCH_TAG, "长按 → 左键按住")
        }
    }

    private fun handleTouch(v: View, ev: MotionEvent): Boolean {
        // 输入绝不能把 UI 线程带崩：任何异常都吞掉并记一条日志
        try {
            dispatchTouch(ev)
        } catch (t: Throwable) {
            Log.w(TOUCH_TAG, "触摸处理异常：$t")
        }
        return true
    }

    private fun dispatchTouch(ev: MotionEvent) {
        val now = SystemClock.uptimeMillis()
        if (cursorX < 0f) {
            cursorX = if (surfaceW > 0) surfaceW / 2f else 720f
            cursorY = if (surfaceH > 0) surfaceH / 2f else 1280f
        }

        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (downs++ < 3) {
                    // 排查用：能打出来就说明触摸确实进了这个 View
                    // （MuMu 那种「应用在独立虚拟 display」的环境里 adb 注入
                    //   根本到不了应用层，只能靠这条日志判断）
                    Log.i(TOUCH_TAG, "ACTION_DOWN @${ev.x},${ev.y} grabbed=${CallbackBridge.grabbed}")
                }
                downX = ev.x
                downY = ev.y
                lastX = ev.x
                lastY = ev.y
                downAt = now
                maxPointers = 1
                moved = false
                leftHeld = false
                multiMoved = false
                scrollAcc = 0f
                ui.postDelayed(longPress, LONG_PRESS_MS)
            }

            MotionEvent.ACTION_POINTER_DOWN -> {
                maxPointers = maxOf(maxPointers, ev.pointerCount)
                // 多指落下就撤销「单指点按/长按」的判定
                ui.removeCallbacks(longPress)
                if (leftHeld) {
                    sendButton(0, false)
                    leftHeld = false
                }
                multiDownAt = now
                multiMoved = false
                lastY = avgY(ev)
                scrollAcc = 0f
            }

            MotionEvent.ACTION_MOVE -> {
                if (ev.pointerCount <= 1) {
                    val dx = ev.x - lastX
                    val dy = ev.y - lastY
                    lastX = ev.x
                    lastY = ev.y
                    if (!moved && hypot(ev.x - downX, ev.y - downY) > TAP_SLOP) {
                        moved = true
                        ui.removeCallbacks(longPress)
                    }
                    if (moved) {
                        moveCursor(dx, dy)
                    }
                } else if (ev.pointerCount >= 2) {
                    val y = avgY(ev)
                    val dy = y - lastY
                    lastY = y
                    if (abs(dy) > 2f) {
                        multiMoved = true
                        multiDownAt = now
                    }
                    scrollAcc += dy
                    while (abs(scrollAcc) >= SCROLL_STEP) {
                        // 手指往上滑 = 滚轮向下（MC 里 = 下一格）
                        val dir = if (scrollAcc > 0f) -1.0 else 1.0
                        sendScroll(dir)
                        scrollAcc -= if (scrollAcc > 0f) SCROLL_STEP else -SCROLL_STEP
                    }
                }
            }

            MotionEvent.ACTION_POINTER_UP -> {
                multiMoved = multiMoved || moved
            }

            MotionEvent.ACTION_UP -> {
                ui.removeCallbacks(longPress)
                val held = now - downAt
                val multiTap = maxPointers >= 2 && !multiMoved && (now - multiDownAt) < TAP_MAX_MS
                when {
                    leftHeld -> {
                        sendButton(0, false)
                        leftHeld = false
                        Log.i(TOUCH_TAG, "松开左键")
                    }
                    multiTap && maxPointers >= 3 -> {
                        sendKey(256) // GLFW_KEY_ESCAPE
                        Log.i(TOUCH_TAG, "三指轻点 → ESC")
                    }
                    multiTap -> {
                        sendButton(1, true)
                        sendButton(1, false)
                        Log.i(TOUCH_TAG, "双指轻点 → 右键")
                    }
                    !moved && held < TAP_MAX_MS -> {
                        sendButton(0, true)
                        sendButton(0, false)
                    }
                }
            }

            MotionEvent.ACTION_CANCEL -> {
                ui.removeCallbacks(longPress)
                if (leftHeld) {
                    sendButton(0, false)
                    leftHeld = false
                }
            }
        }
    }

    private fun avgY(ev: MotionEvent): Float {
        var s = 0f
        for (i in 0 until ev.pointerCount) {
            s += ev.getY(i)
        }
        return if (ev.pointerCount > 0) s / ev.pointerCount else lastY
    }

    /**
     * 移动指针。
     *
     * 抓住鼠标（游戏内）时游戏每帧都会把光标摆回中心，所以这里每次都发
     * 「中心 + 本次增量」；否则增量会被下一帧的归中吃掉一半以上。
     */
    private fun moveCursor(dx: Float, dy: Float) {
        val w = if (surfaceW > 0) surfaceW.toFloat() else 1f
        val h = if (surfaceH > 0) surfaceH.toFloat() else 1f
        if (CallbackBridge.grabbed) {
            val sx = dx * SENS_LOOK
            val sy = dy * SENS_LOOK
            CallbackBridge.nativeSendCursorPos(w / 2f + sx, h / 2f + sy)
        } else {
            cursorX = (cursorX + dx * SENS_CURSOR).coerceIn(1f, w - 1f)
            cursorY = (cursorY + dy * SENS_CURSOR).coerceIn(1f, h - 1f)
            CallbackBridge.nativeSendCursorPos(cursorX, cursorY)
        }
        if (sent++ < 3) {
            Log.i(TOUCH_TAG, "光标 d=($dx,$dy) grabbed=${CallbackBridge.grabbed}")
        }
    }

    private fun sendButton(button: Int, pressed: Boolean) {
        CallbackBridge.nativeSendMouseButton(button, if (pressed) 1 else 0, 0)
        if (sent++ < 3) {
            Log.i(TOUCH_TAG, "鼠标键 $button ${if (pressed) "按下" else "抬起"}")
        }
    }

    private fun sendScroll(y: Double) {
        CallbackBridge.nativeSendScroll(0.0, y)
        if (sent++ < 3) {
            Log.i(TOUCH_TAG, "滚轮 $y")
        }
    }

    private fun sendKey(glfwKey: Int) {
        CallbackBridge.nativeSendKey(glfwKey, 0, 1, 0)
        CallbackBridge.nativeSendKey(glfwKey, 0, 0, 0)
        if (sent++ < 3) {
            Log.i(TOUCH_TAG, "按键 $glfwKey")
        }
    }

    /**
     * 输入自检（联调用）：依次发鼠标/滚轮/按键，结果写进 logcat。
     *
     * 触发方式：`adb shell run-as com.caiwenjun.ccl touch CCL/input-test`，
     * 走 `auto-probe`（`lib.rs` 在探针跑起来后会调这个方法）。
     * 判据：内嵌 JVM 侧随后 `glfwGetCursorPos` 读到 (600,700) 就说明
     * ART 写进去的值确实进了 `pojav_environ`，两个 VM 打通了。
     */
    @JvmStatic
    fun inputSelfTest(): String {
        val b = StringBuilder()
        fun step(name: String, body: () -> Unit) {
            try {
                body()
                b.append(name).append("=ok ")
            } catch (t: Throwable) {
                b.append(name).append("=!").append(t).append(' ')
            }
        }
        step("cursor") { CallbackBridge.nativeSendCursorPos(600f, 700f) }
        step("lbtn") {
            CallbackBridge.nativeSendMouseButton(0, 1, 0)
            CallbackBridge.nativeSendMouseButton(0, 0, 0)
        }
        step("rbtn") {
            CallbackBridge.nativeSendMouseButton(1, 1, 0)
            CallbackBridge.nativeSendMouseButton(1, 0, 0)
        }
        step("scroll") { CallbackBridge.nativeSendScroll(0.0, 1.0) }
        step("key") {
            CallbackBridge.nativeSendKey(256, 0, 1, 0)
            CallbackBridge.nativeSendKey(256, 0, 0, 0)
        }
        val s = b.toString().trim()
        Log.i(TOUCH_TAG, "自检：$s")
        return s
    }
}
