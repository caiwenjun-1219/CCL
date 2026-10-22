//! CCL 自定义前端访问协议：`ccl://`
//!
//! 设计参考 VersePc2 的 `wewp://` 协议（Wallpaper Engine 本地文件服务）：
//! 用 `register_uri_scheme_protocol` 注册一个自定义 scheme，让 WebView 前端
//! 可以像访问 http 资源一样直接读取本地文件（图片/日志/媒体），无需再走
//! `convertFileSrc` 或 IPC 传 base64。
//!
//! 路由：
//! - `ccl://instances/<instance_id>/<相对路径>` → 实例游戏目录下的文件
//!   （如 `ccl://instances/abc-123/screenshots/2026-01-01.png`）
//! - `ccl://data/<相对路径>` → CCL 数据目录下的文件
//!   （如 `ccl://data/settings.json`、`ccl://data/java/...`）
//!
//! 安全与特性：
//! - 路径穿越防护：逐段 percent 解码后拒绝 `.`/`..`，再 canonicalize 前缀校验
//! - MIME 推断（图片/CSS/JS/JSON/字体/音视频等）
//! - HTTP Range 支持（206 + Content-Range），大媒体不会整文件读入内存
//! - CORS 响应头（前端 fetch/`<img>` 均可跨源使用）
//! - OPTIONS 预检直接放行

use std::io::{Read, Seek, SeekFrom};
use tauri::http::{Request, Response, StatusCode};
use tauri::Manager;

/// 单次 Range 响应最大读取 8MiB，避免大文件内存暴涨
const MAX_RANGE_CHUNK: u64 = 8 * 1024 * 1024;

pub fn handle(
    ctx: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let app = ctx.app_handle();

    // 预检直接放行
    if request.method() == "OPTIONS" {
        return cors(Response::builder().status(StatusCode::NO_CONTENT)).body(Vec::new()).unwrap();
    }

    let uri = request.uri().to_string();
    let after_scheme = uri.strip_prefix("ccl://").unwrap_or(&uri);
    // 去掉 host 段（通常为空或 localhost），得到 /instances/... 或 /data/...
    let path_part = match after_scheme.find('/') {
        Some(idx) => &after_scheme[idx..],
        None => "/",
    };
    // 去掉查询参数与开头的 /
    let path_only = path_part.split('?').next().unwrap_or(path_part);
    let path_only = path_only.trim_start_matches('/');

    // 拆成 [root, first, rest]
    let mut segs = path_only.splitn(3, '/');
    let root = segs.next().unwrap_or("");
    let first = segs.next().unwrap_or("");
    let rest = segs.next().unwrap_or("");

    let (base_dir, rel_path): (std::path::PathBuf, String) = match root {
        "instances" => {
            if first.is_empty() || rest.is_empty() {
                return not_found();
            }
            let id = percent_decode(first);
            match crate::instances::load_instances(app).into_iter().find(|i| i.id == id) {
                Some(inst) => (std::path::PathBuf::from(&inst.game_dir), rest.to_string()),
                None => return not_found(),
            }
        }
        "data" => {
            let rp = if rest.is_empty() {
                first.to_string()
            } else {
                format!("{first}/{rest}")
            };
            if rp.is_empty() {
                return not_found();
            }
            (crate::settings::data_dir(app), rp)
        }
        _ => return not_found(),
    };

    // 逐段 percent 解码 + 拒绝路径穿越
    let mut decoded = Vec::new();
    for seg in rel_path.split('/') {
        let d = percent_decode(seg);
        if d == "." || d == ".." {
            return forbidden();
        }
        if d.is_empty() {
            continue;
        }
        decoded.push(d);
    }
    if decoded.is_empty() {
        return not_found();
    }
    let rel_decoded = decoded.join("/");

    let file_path = base_dir.join(&rel_decoded);
    if !file_path.is_file() {
        return not_found();
    }

    // 二次校验：canonicalize 后必须仍在基目录内（防符号链接/编码后的 ..）
    let (Ok(canon_base), Ok(canon_file)) = (base_dir.canonicalize(), file_path.canonicalize()) else {
        return not_found();
    };
    if !canon_file.starts_with(&canon_base) {
        return forbidden();
    }

    let Ok(meta) = std::fs::metadata(&canon_file) else {
        return not_found();
    };
    let len = meta.len();
    let mime = mime_for(&rel_decoded);

    // Range 处理
    let range_header = request
        .headers()
        .get("range")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Some(h) = &range_header {
        let Some((start, end)) = parse_byte_range(h, len) else {
            return cors(Response::builder().status(StatusCode::RANGE_NOT_SATISFIABLE))
                .header("Content-Range", format!("bytes */{len}"))
                .header("Accept-Ranges", "bytes")
                .body(Vec::new())
                .unwrap();
        };
        let mut file = match std::fs::File::open(&canon_file) {
            Ok(f) => f,
            Err(_) => return not_found(),
        };
        let read_len = (end - start + 1).min(MAX_RANGE_CHUNK);
        let end = start + read_len - 1;
        let mut buf = vec![0u8; read_len as usize];
        if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut buf).is_err() {
            return not_found();
        }
        return cors(
            Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header("Content-Type", mime)
                .header("Content-Length", read_len.to_string())
                .header("Content-Range", format!("bytes {start}-{end}/{len}"))
                .header("Accept-Ranges", "bytes"),
        )
        .body(buf)
        .unwrap();
    }

    let Ok(bytes) = std::fs::read(&canon_file) else {
        return not_found();
    };
    cors(
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", mime)
            .header("Content-Length", bytes.len().to_string())
            .header("Accept-Ranges", "bytes"),
    )
    .body(bytes)
    .unwrap()
}

fn cors(builder: tauri::http::response::Builder) -> tauri::http::response::Builder {
    builder
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS")
        .header("Access-Control-Allow-Headers", "Range, Content-Type")
        .header("Access-Control-Expose-Headers", "Content-Range, Content-Length, Accept-Ranges")
        .header("Cross-Origin-Resource-Policy", "cross-origin")
}

fn not_found() -> Response<Vec<u8>> {
    cors(Response::builder().status(StatusCode::NOT_FOUND)).body(Vec::new()).unwrap()
}

fn forbidden() -> Response<Vec<u8>> {
    cors(Response::builder().status(StatusCode::FORBIDDEN)).body(Vec::new()).unwrap()
}

fn percent_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .to_string()
}

/// 解析 `bytes=start-end` / `bytes=start-` / `bytes=-suffix`，返回闭区间
fn parse_byte_range(header: &str, len: u64) -> Option<(u64, u64)> {
    if len == 0 {
        return None;
    }
    let spec = header.trim().strip_prefix("bytes=")?;
    let (s, e) = spec.split_once('-')?;
    if s.trim().is_empty() {
        // 后缀范围：最后 N 字节
        let n: u64 = e.trim().parse().ok()?;
        if n == 0 {
            return None;
        }
        let start = len.saturating_sub(n);
        Some((start, len - 1))
    } else {
        let start: u64 = s.trim().parse().ok()?;
        if start >= len {
            return None;
        }
        let end = if e.trim().is_empty() {
            len - 1
        } else {
            e.trim().parse::<u64>().ok()?.min(len - 1)
        };
        if end < start {
            return None;
        }
        Some((start, end))
    }
}

fn mime_for(path: &str) -> &'static str {
    let ext = path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" | "cjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "xml" => "text/xml; charset=utf-8",
        "txt" | "md" | "log" | "cfg" | "properties" | "toml" | "yml" | "yaml" => {
            "text/plain; charset=utf-8"
        }
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "zip" => "application/zip",
        "jar" => "application/java-archive",
        _ => "application/octet-stream",
    }
}
