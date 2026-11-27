package com.caiwenjun.ccl

import android.os.Bundle
import android.util.Log
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    // 游戏画面是否「盖在 WebView 之上」。
    //
    // 只能在这里一次性定死：运行期改 z-order 会让 SurfaceView 重建 Surface，
    // 已经桥接给 libpojavexec 的 ANativeWindow 就指向一块死内存（详见 GameSurface）。
    //
    // 调试用：adb shell am start -n com.caiwenjun.ccl/.MainActivity --ez ccl_overlay true
    val overlay = intent?.getBooleanExtra("ccl_overlay", false) ?: false
    Log.i("CCLMain", "onCreate overlay=$overlay")

    // 必须在 super.onCreate 之后：Tauri 的 WebView 是在那里 setContentView 的，
    // 提前挂 SurfaceView 会因为窗口还没建好而失败。
    GameSurface.attach(this, overlay)
  }

  override fun onResume() {
    super.onResume()
    // 兜底：Activity 重建或被某些 ROM 重组窗口后，把 SurfaceView 挂回去
    GameSurface.ensureAttached(this)
  }
}
