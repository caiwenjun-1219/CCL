"""把本地已验证的 JRE 包同步进 APK assets（内置 Java 运行时）。

用法：
    python sync_jre_assets.py            # 从 jre_cache/ 同步到插件 assets
    python sync_jre_assets.py --check    # 只校验，不复制

为什么单独一步：
`jre17-universal.tar.xz`（21 MB）+ `jre17-bin-arm64.tar.xz`（4.9 MB）体积太大，
不入 git；构建前先跑本脚本，把校验通过的包放到
`src-tauri/plugins/ccl-android/android/src/main/assets/jre/`，
AGP 会把库模块的 assets 合并进 APK，首启由 Kotlin 侧拷出来解包（见
`src-tauri/src/java/android.rs` 的 install_bundled）。

缺少任一个包时，App 行为不变：`bundled_java_major()` 返回 None，
首启自动退回在线下载。
"""
import hashlib
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
CACHE = ROOT / "jre_cache"
ASSETS = ROOT / "src-tauri/plugins/ccl-android/android/src/main/assets/jre"

# 与 https://github.com/SouhardyaMaybe/EclipseJRE/releases/tag/jres-v1.0.0
# 的 jre17-multiarch.json 一致
WANT = {
    "jre17-universal.tar.xz": (
        "e84d33561ebd19c004c31d3e04a1ea9af95d906cd6fda6778a63f8ed218dffb2",
        22008312,
    ),
    "jre17-bin-arm64.tar.xz": (
        "63d3c8ea62692d1d30fffed407ce23ed5095e895f395b28ead0061699be84456",
        5142364,
    ),
}


def sha256_of(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def main(check_only: bool) -> int:
    ASSETS.mkdir(parents=True, exist_ok=True)
    missing = []
    for name, (sha, size) in WANT.items():
        src = CACHE / name
        if not src.exists() or src.stat().st_size != size:
            print(f"✗ {name} 缺失或大小不符（{src}）")
            missing.append(name)
            continue
        got = sha256_of(src)
        if got != sha:
            print(f"✗ {name} sha256 不符：{got}")
            missing.append(name)
            continue
        dst = ASSETS / name
        if check_only:
            ok = dst.exists() and dst.stat().st_size == size
            print(f"{'✓' if ok else '·'} {name} 校验通过；assets {'已存在' if ok else '未同步'}")
            continue
        shutil.copy2(src, dst)
        print(f"✓ {name} → {dst.relative_to(ROOT)}")

    if missing:
        print("\n未同步完整：APK 将不含内置 JRE，首启会退回在线下载。")
        print("拉包脚本：python pull_universal.py（单连接续传，多镜像轮换）")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main("--check" in sys.argv))
