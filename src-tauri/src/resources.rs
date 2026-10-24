//! 实例资源管理：存档 / 资源包 / 光影包（对齐 PCL 的版本资源页）
//!
//! - 存档：目录名 + level.dat 解析出的世界名与图标（NBT 自解析，gzip 解压）
//! - 资源包：resourcepacks/*.zip|目录，读 pack.mcmeta 取描述，启用状态读 options.txt
//! - 光影包：shaderpacks/*.zip|目录，启用项为 options.txt 的 shaderPack

use serde::Serialize;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::instances::{get_instance, Instance};

/* ------------------------------------------------------------------ 目录 */

fn game_dir(app: &AppHandle, instance_id: &str) -> Result<PathBuf, String> {
    Ok(PathBuf::from(get_instance(app, instance_id)?.game_dir))
}

fn subdir(inst: &Instance, name: &str) -> PathBuf {
    PathBuf::from(&inst.game_dir).join(name)
}

/// 递归统计目录占用（字节）
fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(rd) = fs::read_dir(path) else {
        return 0;
    };
    for e in rd.flatten() {
        let p = e.path();
        match fs::symlink_metadata(&p) {
            Ok(md) if md.is_dir() => total += dir_size(&p),
            Ok(md) => total += md.len(),
            Err(_) => {}
        }
    }
    total
}

fn mtime_secs(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[tauri::command]
pub fn open_game_subdir(app: AppHandle, instance_id: String, sub: String) -> Result<(), String> {
    let inst = get_instance(&app, &instance_id)?;
    let dir = subdir(&inst, &sub);
    fs::create_dir_all(&dir).ok();
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn instance_size(app: AppHandle, instance_id: String) -> Result<u64, String> {
    let inst = get_instance(&app, &instance_id)?;
    Ok(dir_size(Path::new(&inst.game_dir)))
}

/* ------------------------------------------------------------------ NBT */

/// 极简 NBT 读取器：只用来从 level.dat 取 LevelName / Icon
struct Cur<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, p: 0 }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.p.checked_add(n)?;
        if end > self.b.len() {
            return None;
        }
        let s = &self.b[self.p..end];
        self.p = end;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|s| s[0])
    }
    fn i16(&mut self) -> Option<i16> {
        self.take(2).map(|s| i16::from_be_bytes([s[0], s[1]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.take(4).map(|s| i32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn string(&mut self) -> Option<String> {
        let n = self.i16()? as usize;
        let s = self.take(n)?;
        Some(String::from_utf8_lossy(s).to_string())
    }

    /// 跳过一个指定类型的 payload
    fn skip(&mut self, ty: u8) -> Option<()> {
        match ty {
            0 => Some(()),
            1 => self.take(1).map(|_| ()),
            2 => self.take(2).map(|_| ()),
            3 | 5 => self.take(4).map(|_| ()),
            4 | 6 => self.take(8).map(|_| ()),
            7 => {
                let n = self.i32()?;
                if n < 0 {
                    return None;
                }
                self.take(n as usize).map(|_| ())
            }
            8 => {
                let n = self.i16()?;
                if n < 0 {
                    return None;
                }
                self.take(n as usize).map(|_| ())
            }
            9 => {
                let elem = self.u8()?;
                let n = self.i32()?;
                if n < 0 {
                    return None;
                }
                for _ in 0..n {
                    self.skip(elem)?;
                }
                Some(())
            }
            10 => {
                loop {
                    let t = self.u8()?;
                    if t == 0 {
                        return Some(());
                    }
                    self.string()?;
                    self.skip(t)?;
                }
            }
            11 => {
                let n = self.i32()?;
                if n < 0 {
                    return None;
                }
                self.take((n as usize).checked_mul(4)?).map(|_| ())
            }
            12 => {
                let n = self.i32()?;
                if n < 0 {
                    return None;
                }
                self.take((n as usize).checked_mul(8)?).map(|_| ())
            }
            _ => None,
        }
    }
}

#[derive(Default)]
struct LevelMeta {
    name: Option<String>,
    icon: Option<Vec<u8>>,
}

/// 递归扫描 compound，收集 LevelName 与 Icon（限深，防病态文件）
fn scan_compound(cur: &mut Cur, depth: u32, out: &mut LevelMeta) -> Option<()> {
    loop {
        let ty = cur.u8()?;
        if ty == 0 {
            return Some(());
        }
        let name = cur.string()?;
        match ty {
            10 => {
                if depth < 4 {
                    scan_compound(cur, depth + 1, out)?;
                } else {
                    cur.skip(10)?;
                }
            }
            8 => {
                let s = cur.string()?;
                if name == "LevelName" && out.name.is_none() && !s.trim().is_empty() {
                    out.name = Some(s);
                }
            }
            7 => {
                let n = cur.i32()?;
                if n < 0 {
                    return None;
                }
                let data = cur.take(n as usize)?;
                if name == "Icon" && out.icon.is_none() {
                    out.icon = Some(data.to_vec());
                }
            }
            _ => cur.skip(ty)?,
        }
    }
}

fn read_level_meta(save_dir: &Path) -> LevelMeta {
    let mut out = LevelMeta::default();
    let Ok(raw) = fs::read(save_dir.join("level.dat")) else {
        return out;
    };
    let mut buf = Vec::new();
    let mut dec = flate2::read::GzDecoder::new(Cursor::new(&raw));
    if dec.read_to_end(&mut buf).is_err() {
        return out;
    }
    let mut cur = Cur::new(&buf);
    // 根：tag type(10) + name + compound
    if cur.u8() == Some(10) {
        let _ = cur.string();
        let _ = scan_compound(&mut cur, 0, &mut out);
    }
    out
}

/* ------------------------------------------------------------------ 存档 */

#[derive(Debug, Serialize)]
pub struct SaveInfo {
    /// 存档目录名（Saves 下的文件夹名，操作时用作 key）
    pub dir_name: String,
    /// level.dat 里的世界名
    pub level_name: String,
    pub size: u64,
    /// 最后修改时间（unix 秒）
    pub mtime: i64,
    /// data:image/png;base64,...（level.dat 内嵌图标）
    pub icon: Option<String>,
}

#[tauri::command]
pub fn list_saves(app: AppHandle, instance_id: String) -> Result<Vec<SaveInfo>, String> {
    let inst = get_instance(&app, &instance_id)?;
    let dir = subdir(&inst, "saves");
    let mut out: Vec<SaveInfo> = Vec::new();
    let Ok(rd) = fs::read_dir(&dir) else {
        return Ok(out);
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        // 只认含 level.dat 的目录，避免把备份/垃圾目录当存档
        if !p.join("level.dat").exists() {
            continue;
        }
        let dir_name = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if dir_name.is_empty() {
            continue;
        }
        let meta = read_level_meta(&p);
        let icon = meta.icon.filter(|v| v.len() > 8).map(|v| {
            use base64::Engine;
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(v)
            )
        });
        out.push(SaveInfo {
            level_name: meta.name.unwrap_or_else(|| dir_name.clone()),
            dir_name,
            size: dir_size(&p),
            mtime: mtime_secs(&p),
            icon,
        });
    }
    out.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    Ok(out)
}

#[tauri::command]
pub fn delete_save(app: AppHandle, instance_id: String, dir_name: String) -> Result<(), String> {
    if dir_name.contains("..") || dir_name.contains('/') || dir_name.contains('\\') {
        return Err("非法的存档目录名".into());
    }
    let inst = get_instance(&app, &instance_id)?;
    let p = subdir(&inst, "saves").join(&dir_name);
    if !p.is_dir() {
        return Err("存档不存在".into());
    }
    fs::remove_dir_all(&p).map_err(|e| format!("删除存档失败: {e}"))
}

/// 备份存档到 <game>/saves/../backup_saves/<存档名>-<时间戳>.zip
#[tauri::command]
pub fn backup_save(app: AppHandle, instance_id: String, dir_name: String) -> Result<String, String> {
    if dir_name.contains("..") || dir_name.contains('/') || dir_name.contains('\\') {
        return Err("非法的存档目录名".into());
    }
    let inst = get_instance(&app, &instance_id)?;
    let src = subdir(&inst, "saves").join(&dir_name);
    if !src.is_dir() {
        return Err("存档不存在".into());
    }
    let out_dir = PathBuf::from(&inst.game_dir).join("backup_saves");
    fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let safe: String = dir_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect();
    let dest = out_dir.join(format!("{safe}-{stamp}.zip"));
    let file = fs::File::create(&dest).map_err(|e| format!("创建备份文件失败: {e}"))?;
    let mut zw = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    fn walk(zw: &mut zip::ZipWriter<fs::File>, root: &Path, cur: &Path, opts: zip::write::SimpleFileOptions) -> Result<(), String> {
        let Ok(rd) = fs::read_dir(cur) else { return Ok(()) };
        for e in rd.flatten() {
            let p = e.path();
            let rel = p
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            if p.is_dir() {
                zw.add_directory(format!("{rel}/"), opts).ok();
                walk(zw, root, &p, opts)?;
            } else if let Ok(data) = fs::read(&p) {
                zw.start_file(rel, opts).map_err(|e| e.to_string())?;
                use std::io::Write;
                zw.write_all(&data).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    walk(&mut zw, &src, &src, opts)?;
    zw.finish().map_err(|e| format!("写入备份失败: {e}"))?;
    Ok(dest.to_string_lossy().to_string())
}

/* ------------------------------------------------- options.txt 读写 */

const OPTIONS: &str = "options.txt";

fn read_options(inst: &Instance) -> Vec<String> {
    fs::read_to_string(subdir(inst, OPTIONS))
        .map(|t| t.lines().map(|s| s.to_string()).collect())
        .unwrap_or_default()
}

fn write_options(inst: &Instance, lines: &[String]) -> Result<(), String> {
    let text = lines.join("\n");
    fs::write(subdir(inst, OPTIONS), text).map_err(|e| format!("写入 options.txt 失败: {e}"))
}

/// 取 key 的值（options.txt 用 `key:value`，值可能含冒号）
fn opt_get<'a>(lines: &'a [String], key: &str) -> Option<&'a str> {
    let prefix = format!("{key}:");
    lines
        .iter()
        .find_map(|l| l.strip_prefix(&prefix))
}

fn opt_set(lines: &mut Vec<String>, key: &str, value: &str) {
    let prefix = format!("{key}:");
    if let Some(slot) = lines.iter_mut().find(|l| l.starts_with(&prefix)) {
        *slot = format!("{prefix}{value}");
    } else {
        lines.push(format!("{prefix}{value}"));
    }
}

/// 资源包启用列表（options.txt: resourcePacks:["vanilla","file/x.zip"]）
fn read_pack_list(inst: &Instance) -> Vec<String> {
    let lines = read_options(inst);
    let raw = opt_get(&lines, "resourcePacks").unwrap_or("[]");
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_else(|_| vec!["vanilla".to_string()])
}

fn write_pack_list(inst: &Instance, list: &[String]) -> Result<(), String> {
    let mut lines = read_options(inst);
    let mut list: Vec<String> = list.to_vec();
    // vanilla 必须始终在最前
    list.retain(|s| s != "vanilla");
    list.insert(0, "vanilla".to_string());
    let json = serde_json::to_string(&list).map_err(|e| e.to_string())?;
    opt_set(&mut lines, "resourcePacks", &json);
    write_options(inst, &lines)
}

/* ------------------------------------------------------- 资源包 / 光影 */

#[derive(Debug, Serialize)]
pub struct PackInfo {
    pub file_name: String,
    /// 展示名（pack.mcmeta 的 pack.description，取不到则用文件名）
    pub display_name: String,
    pub description: String,
    pub size: u64,
    pub mtime: i64,
    pub enabled: bool,
    pub is_dir: bool,
    /// pack_format 与当前版本不符（Minecraft 会在游戏内标警告）
    pub incompatible: bool,
}

fn read_mcmeta_from_zip(path: &Path) -> Option<Vec<u8>> {
    let f = fs::File::open(path).ok()?;
    let mut zf = zip::ZipArchive::new(f).ok()?;
    let mut entry = zf.by_name("pack.mcmeta").ok()?;
    let mut s = String::new();
    entry.read_to_string(&mut s).ok()?;
    Some(s.into_bytes())
}

/// 从 zip 或目录里读 pack.mcmeta 的 description
fn read_pack_meta(path: &Path) -> (Option<String>, Option<i64>) {
    let raw = if path.is_dir() {
        fs::read(path.join("pack.mcmeta")).ok()
    } else {
        read_mcmeta_from_zip(path)
    };
    let Some(bytes) = raw else {
        return (None, None);
    };
    let text = String::from_utf8_lossy(&bytes).to_string();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return (None, None);
    };
    let pack = match v.get("pack") {
        Some(p) => p,
        None => return (None, None),
    };
    // description 可能是字符串，也可能是 raw JSON 文本组件（1.13+）
    let desc = match pack.get("description") {
        Some(serde_json::Value::String(s)) => Some(strip_mc_codes(s)),
        Some(other) => Some(json_text_component(&other.to_string())),
        None => None,
    };
    let fmt = pack.get("pack_format").and_then(|f| f.as_i64());
    (desc, fmt)
}

/// 去掉 § 颜色/格式代码（§a、§l 等）
fn strip_mc_codes(s: &str) -> String {
    let mut clean = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            clean.push(c);
        }
    }
    clean.trim().to_string()
}

/// 从 {"text":"A","extra":[{"text":"B"}]} 之类的文本组件里拼出纯文本
fn json_text_component(s: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(s) else {
        return s.trim_matches('"').to_string();
    };
    fn collect(v: &serde_json::Value, out: &mut String) {
        match v {
            serde_json::Value::String(s) => out.push_str(s),
            serde_json::Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
            serde_json::Value::Object(o) => {
                // 带 translate 的组件用 fallback 作为可读文本
                if let Some(t) = o.get("fallback").and_then(|t| t.as_str()) {
                    out.push_str(t);
                } else if let Some(t) = o.get("text").and_then(|t| t.as_str()) {
                    out.push_str(t);
                }
                if let Some(e) = o.get("extra") {
                    collect(e, out);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    collect(&v, &mut out);
    strip_mc_codes(&out)
}

/// 该 MC 版本对应的资源包 pack_format（粗略映射，仅用于「不兼容」提示）
fn expected_pack_format(mc: &str) -> Option<i64> {
    let v = mc.trim_start_matches("1.");
    let minor: i64 = v.split('.').next().and_then(|s| s.parse().ok())?;
    Some(match minor {
        21 => 34,
        20 => 22,
        19 => 15,
        18 => 9,
        17 => 8,
        16 => 6,
        15 => 5,
        _ => return None,
    })
}

fn collect_packs(dir: &Path, expected: Option<i64>) -> Vec<PackInfo> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        let is_dir = p.is_dir();
        if is_dir {
            if !p.join("pack.mcmeta").exists() {
                continue;
            }
        } else {
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext != "zip" {
                continue;
            }
        }
        let Some(file_name) = p.file_name().and_then(|s| s.to_str()).map(|s| s.to_string()) else {
            continue;
        };
        let (desc, fmt) = read_pack_meta(&p);
        let size = if is_dir { dir_size(&p) } else { fs::metadata(&p).map(|m| m.len()).unwrap_or(0) };
        let incompatible = match (expected, fmt) {
            (Some(exp), Some(f)) => f > exp,
            _ => false,
        };
        out.push(PackInfo {
            display_name: desc.clone().unwrap_or_else(|| file_name.clone()),
            description: desc.unwrap_or_default(),
            file_name,
            size,
            mtime: mtime_secs(&p),
            enabled: false,
            is_dir,
            incompatible,
        });
    }
    out.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    out
}

#[tauri::command]
pub fn list_resourcepacks(app: AppHandle, instance_id: String) -> Result<Vec<PackInfo>, String> {
    let inst = get_instance(&app, &instance_id)?;
    let expected = expected_pack_format(&inst.minecraft_version);
    let mut packs = collect_packs(&subdir(&inst, "resourcepacks"), expected);
    let active = read_pack_list(&inst);
    for p in packs.iter_mut() {
        p.enabled = active
            .iter()
            .any(|a| a == &p.file_name || a == &format!("file/{}", p.file_name));
    }
    // 已启用的排前面，其次按修改时间
    packs.sort_by(|a, b| b.enabled.cmp(&a.enabled).then(b.mtime.cmp(&a.mtime)));
    Ok(packs)
}

#[tauri::command]
pub fn toggle_resourcepack(
    app: AppHandle,
    instance_id: String,
    file_name: String,
    enabled: bool,
) -> Result<(), String> {
    let inst = get_instance(&app, &instance_id)?;
    let mut list = read_pack_list(&inst);
    let key = format!("file/{file_name}");
    list.retain(|a| a != &file_name && a != &key);
    if enabled {
        list.push(key);
    }
    write_pack_list(&inst, &list)
}

#[tauri::command]
pub fn delete_resourcepack(app: AppHandle, instance_id: String, file_name: String) -> Result<(), String> {
    if file_name.contains("..") || file_name.contains('/') || file_name.contains('\\') {
        return Err("非法的文件名".into());
    }
    let inst = get_instance(&app, &instance_id)?;
    // 从启用列表移除
    let mut list = read_pack_list(&inst);
    let key = format!("file/{file_name}");
    if list.iter().any(|a| a == &file_name || a == &key) {
        list.retain(|a| a != &file_name && a != &key);
        write_pack_list(&inst, &list)?;
    }
    let p = subdir(&inst, "resourcepacks").join(&file_name);
    if p.is_dir() {
        fs::remove_dir_all(&p).map_err(|e| format!("删除失败: {e}"))
    } else if p.exists() {
        fs::remove_file(&p).map_err(|e| format!("删除失败: {e}"))
    } else {
        Err("资源包不存在".into())
    }
}

#[tauri::command]
pub fn list_shaderpacks(app: AppHandle, instance_id: String) -> Result<Vec<PackInfo>, String> {
    let inst = get_instance(&app, &instance_id)?;
    let mut packs = collect_packs(&subdir(&inst, "shaderpacks"), None);
    let lines = read_options(&inst);
    let current = opt_get(&lines, "shaderPack").unwrap_or("OFF").to_string();
    for p in packs.iter_mut() {
        p.enabled = current == p.file_name;
        p.incompatible = false;
    }
    Ok(packs)
}

/// 启用指定光影；name 传空或 "OFF" 表示关闭光影
#[tauri::command]
pub fn set_shaderpack(app: AppHandle, instance_id: String, name: String) -> Result<(), String> {
    let inst = get_instance(&app, &instance_id)?;
    let mut lines = read_options(&inst);
    let val = if name.trim().is_empty() || name == "OFF" {
        "OFF".to_string()
    } else {
        name
    };
    opt_set(&mut lines, "shaderPack", &val);
    write_options(&inst, &lines)
}

#[tauri::command]
pub fn delete_shaderpack(app: AppHandle, instance_id: String, file_name: String) -> Result<(), String> {
    if file_name.contains("..") || file_name.contains('/') || file_name.contains('\\') {
        return Err("非法的文件名".into());
    }
    let inst = get_instance(&app, &instance_id)?;
    let lines = read_options(&inst);
    if opt_get(&lines, "shaderPack") == Some(file_name.as_str()) {
        set_shaderpack(app.clone(), instance_id.clone(), "OFF".into())?;
    }
    let p = subdir(&inst, "shaderpacks").join(&file_name);
    if p.is_dir() {
        fs::remove_dir_all(&p).map_err(|e| format!("删除失败: {e}"))
    } else if p.exists() {
        fs::remove_file(&p).map_err(|e| format!("删除失败: {e}"))
    } else {
        Err("光影包不存在".into())
    }
}

/// 导入本地资源包/光影包文件（zip）到对应目录
#[tauri::command]
pub fn import_pack_file(
    app: AppHandle,
    instance_id: String,
    kind: String,
    src_path: String,
) -> Result<String, String> {
    let inst = get_instance(&app, &instance_id)?;
    let folder = match kind.as_str() {
        "resourcepack" => "resourcepacks",
        "shaderpack" => "shaderpacks",
        _ => return Err("未知的资源类型".into()),
    };
    let src = PathBuf::from(&src_path);
    let file_name = src
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("无效的文件路径")?
        .to_string();
    let dest_dir = subdir(&inst, folder);
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(&file_name);
    fs::copy(&src, &dest).map_err(|e| format!("复制失败: {e}"))?;
    Ok(file_name)
}

/* ------------------------------------------------------------------ 日志 */

#[derive(Debug, Serialize)]
pub struct LogFile {
    pub name: String,
    /// 文件大小
    pub size: u64,
    pub mtime: i64,
}

/// 列出 logs/ 下的日志文件（新→旧），含崩溃报告 crash-reports/
#[tauri::command]
pub fn list_game_logs(app: AppHandle, instance_id: String) -> Result<Vec<LogFile>, String> {
    let inst = get_instance(&app, &instance_id)?;
    let mut out = Vec::new();
    for (folder, only_crash) in [("logs", false), ("crash-reports", true)] {
        let dir = subdir(&inst, folder);
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !name.ends_with(".log") && !name.ends_with(".txt") && !name.ends_with(".gz") {
                continue;
            }
            if only_crash && !name.contains("crash") {
                continue;
            }
            let size = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            out.push(LogFile {
                name: if only_crash {
                    format!("crash-reports/{name}")
                } else {
                    name.to_string()
                },
                size,
                mtime: mtime_secs(&p),
            });
        }
    }
    out.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    out.truncate(40);
    Ok(out)
}

/// 读取日志尾部（最多 tail 行，避免超大文件卡界面）
#[tauri::command]
pub fn read_game_log(app: AppHandle, instance_id: String, name: String, tail: usize) -> Result<String, String> {
    if name.contains("..") {
        return Err("非法的日志名".into());
    }
    let inst = get_instance(&app, &instance_id)?;
    let path = if name.contains('/') || name.contains('\\') {
        PathBuf::from(&inst.game_dir).join(&name)
    } else {
        subdir(&inst, "logs").join(&name)
    };
    if !path.is_file() {
        return Err("日志文件不存在".into());
    }
    let text = if path.extension().and_then(|s| s.to_str()) == Some("gz") {
        let raw = fs::read(&path).map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        flate2::read::GzDecoder::new(Cursor::new(&raw))
            .read_to_end(&mut buf)
            .map_err(|e| format!("解压日志失败: {e}"))?;
        String::from_utf8_lossy(&buf).to_string()
    } else {
        fs::read_to_string(&path).map_err(|e| e.to_string())?
    };
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(tail.max(1));
    Ok(lines[start..].join("\n"))
}
