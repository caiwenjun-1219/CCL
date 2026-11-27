# CCL Android 插件：release 构建开启了 R8（optimization.enable = true）。
#
# tauri-api 自带的 consumer 规则只保 `@TauriPlugin` 类里被注解的方法，
# 不保插件自己的参数类。参数类字段一旦被重命名，Jackson 按字段名反序列化就会
# 拿到空值（表现为：命令"成功"返回、但参数全是空串，静默失效）。
#
# 因此这里把本插件包整体保留，彻底避免 R8 动插件代码。
-keep class com.caiwenjun.ccl.android.** { *; }

# 兜底：任何插件里带 @InvokeArg 的参数类都保字段名
-keep @app.tauri.annotation.InvokeArg class * { *; }

# 兜底：@Command / @ActivityCallback 方法名不能被改（Rust 侧按名字调用）
-keepclassmembers class * {
    @app.tauri.annotation.Command <methods>;
    @app.tauri.annotation.ActivityCallback <methods>;
    @app.tauri.annotation.PermissionCallback <methods>;
}
