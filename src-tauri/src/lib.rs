// Cwj Craft Launcher (CCL) - 库入口（桌面 / Android 共用）
// Tauri 2 标准结构：lib.rs 承载全部模块与 run()，桌面 main.rs 与 Android
// 原生入口都调用这里。Android 由 tauri::mobile_entry_point 生成 so 入口。

mod auth;
mod curseforge;
mod downloader;
mod instances;
mod java;
mod launcher;
mod minecraft;
mod modpack;
mod modrinth;
mod paths;
mod protocol;
mod proxy;
mod resources;
mod settings;
mod updater;

use launcher::GameProcesses;
use std::collections::HashMap;
use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // Android 存储权限（MANAGE_EXTERNAL_STORAGE）申请与检测；桌面端为空操作
        .plugin(ccl_android::init())
        // ccl:// 自定义协议：为前端提供本地文件服务（实例截图/数据目录文件），
        // 支持 Range 与 CORS，详见 protocol.rs
        .register_uri_scheme_protocol("ccl", protocol::handle)
        .manage(GameProcesses(Arc::new(std::sync::Mutex::new(HashMap::new()))))
        .invoke_handler(tauri::generate_handler![
            // 设置
            settings::get_settings,
            settings::save_settings,
            // Java
            java::scan_javas,
            java::download_java,
            java::resolve_java,
            java::java_env,
            java::java_smoke_test,
            java::java_embedded_test,
            java::java_lwjgl_probe,
            java::android_launch_plan,
            java::android_launch_game,
            // Minecraft 版本
            minecraft::mc_version_manifest,
            minecraft::mc_local_versions,
            minecraft::mc_delete_version,
            minecraft::mc_install_version,
            minecraft::mc_download_as_instance,
            minecraft::mc_test_version,
            minecraft::loader_versions,
            minecraft::mc_install_loader,
            minecraft::forge_versions,
            minecraft::mc_install_forge,
            // 实例
            instances::list_instances,
            instances::create_instance,
            instances::update_instance,
            instances::delete_instance,
            instances::open_instance_dir,
            instances::migrate_instances_to_mc_folder,
            instances::instance_paths,
            paths::scan_game_folders,
            paths::get_mc_folder,
            paths::add_custom_game_folder,
            paths::remove_custom_game_folder,
            paths::switch_game_folder,
            instances::list_screenshots,
            instances::list_mods,
            instances::toggle_mod,
            instances::delete_mod,
            instances::install_mod_file,
            instances::open_mods_dir,
            // 实例资源（存档 / 资源包 / 光影）
            resources::open_game_subdir,
            resources::instance_size,
            resources::list_saves,
            resources::delete_save,
            resources::backup_save,
            resources::list_resourcepacks,
            resources::toggle_resourcepack,
            resources::delete_resourcepack,
            resources::list_shaderpacks,
            resources::set_shaderpack,
            resources::delete_shaderpack,
            resources::import_pack_file,
            resources::list_game_logs,
            resources::read_game_log,
            // 启动
            launcher::launch_instance,
            launcher::stop_instance,
            launcher::is_instance_running,
            // 账户
            auth::accounts_list,
            auth::remove_account,
            auth::ms_login_start,
            auth::ms_login_poll,
            auth::ms_login_exchange,
            auth::offline_login,
            auth::refresh_account,
            // 模组市场
            modrinth::modrinth_search,
            modrinth::modrinth_recommend,
            modrinth::modrinth_versions,
            modrinth::modrinth_install,
            modrinth::modrinth_install_resource,
            modrinth::modrinth_install_modpack,
            curseforge::cf_search,
            curseforge::cf_files,
            curseforge::cf_install,
            // 整合包
            modpack::import_modpack,
            modpack::export_modpack,
            // 云控更新
            updater::app_version,
            updater::check_cloud_update,
            updater::install_cloud_update,
            // 网络代理
            proxy::proxy_detect,
            proxy::proxy_test,
        ])
        .setup(|app| {
            // 启动时按设置初始化全局代理（影响所有 HTTP 请求）
            let s = settings::load(app.handle());
            proxy::set_proxy(&s.proxy);

            // 联调入口：数据目录下的开关文件决定启动几秒后自动跑什么，结果打到 logcat。
            //
            // 为什么不用按钮：MuMu 多 display 下 uiautomator 抓不到应用层，点不准。
            // 生产环境不会有这些文件，等于零成本。
            #[cfg(target_os = "android")]
            {
                let data = settings::data_dir(app.handle());
                // 三个联调开关**互斥**：内嵌 JVM 一个进程只能建一次，探针会用
                // 「三个 jar」的 classpath 把 VM 占住，真启动就再也没机会了。
                // 所以做成 else-if 链，auto-launch 优先级最高。
                //
                // 生产环境这三个文件都不存在，等于零成本。要用就：
                //   adb shell run-as com.caiwenjun.ccl touch CCL/auto-launch
                if data.join("auto-launch").exists() {
                    let h = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(5));
                        let all = crate::instances::load_instances(&h);
                        let Some(inst) = all.first() else {
                            println!("[launch] 没有实例，跳过");
                            return;
                        };
                        println!("[launch] 试启动实例 {}（{}）", inst.name, inst.id);
                        match java::launch::start(&h, &inst.id) {
                            Ok(r) => {
                                println!("[launch] {}", r.verdict);
                                println!(
                                    "[launch] java_home={} vm_ok={} vm_err={} surface={}",
                                    r.java_home, r.vm_ok, r.vm_error, r.surface_info
                                );
                            }
                            Err(e) => println!("[launch] ERR {e}"),
                        }
                    });
                } else if data.join("auto-probe").exists() {
                    let h = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(6));
                        let r = tauri::async_runtime::block_on(java::lwjgl::run(h));
                        match r {
                            Ok(r) => {
                                println!("[probe] verdict={}", r.verdict);
                                println!(
                                    "[probe] dir={} abi={} so={} ms={} cached={}",
                                    r.dir, r.abi, r.native_count, r.prepare_ms, r.prepared_cached
                                );
                                println!("[probe] vm_ok={} vm_err={}", r.vm_ok, r.vm_error);
                                println!(
                                    "[probe] surface_ok={} info={}",
                                    r.surface_ok, r.surface_info
                                );
                                println!("[probe] cp={}", r.classpath);
                                for line in r.probe.lines() {
                                    println!("[probe] probe {line}");
                                }
                            }
                            Err(e) => println!("[probe] ERR {e}"),
                        }
                    });
                } else if data.join("auto-plan").exists() {
                    let h = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        let all = crate::instances::load_instances(&h);
                        println!("[plan] 实例数={}", all.len());
                        let Some(inst) = all.first() else {
                            println!("[plan] 没有实例，跳过");
                            return;
                        };
                        match java::launch::plan(&h, &inst.id) {
                            Ok(p) => {
                                println!(
                                    "[plan] version={} main={} vm_ready={}",
                                    p.version_id, p.main_class, p.vm_ready
                                );
                                println!("[plan] 条目数={}", p.entries.len());
                                for (i, e) in p.entries.iter().enumerate().take(6) {
                                    println!("[plan] cp[{i}]={e}");
                                }
                                for n in &p.notes {
                                    println!("[plan] note={n}");
                                }
                                println!("[plan] jvm={}", p.jvm_args.join(" "));
                                println!("[plan] game={}", p.game_args.join(" "));
                            }
                            Err(e) => println!("[plan] ERR {e}"),
                        }
                    });
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running CCL");
}
