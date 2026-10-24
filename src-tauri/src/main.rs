// Cwj Craft Launcher (CCL) - Tauri 入口

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
mod resources;
mod settings;
mod updater;

use launcher::GameProcesses;
use std::collections::HashMap;
use std::sync::Arc;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running CCL");
}
