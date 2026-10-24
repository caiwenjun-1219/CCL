// Cwj Craft Launcher (CCL) - 桌面端入口
// 全部逻辑在 lib.rs（桌面与 Android 共用），此处仅作 Windows 子系统的薄入口。

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

fn main() {
    ccl_lib::run()
}
