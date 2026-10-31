package com.caiwenjun.ccl

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // 必须在 super.onCreate 之后：Tauri 的 WebView 是在那里 setContentView 的，
    // 提前挂 SurfaceView 会因为窗口还没建好而失败。
    GameSurface.attach(this)
  }

  override fun onResume() {
    super.onResume()
    // 兜底：Activity 重建或被某些 ROM 重组窗口后，把 SurfaceView 挂回去
    GameSurface.ensureAttached(this)
  }
}
