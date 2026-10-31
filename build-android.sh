#!/usr/bin/env bash
# CCL 安卓构建脚本
#
# 为什么需要它：tauri CLI 在 Windows 上会把 libccl_lib.so「符号链接」进 jniLibs，
# 而建符号链接需要开发者模式/管理员权限，普通账号必失败：
#   Failed to create a symbolic link ... Creation symbolic link is not allowed
# 所以这里改成手工三步：cargo 编 .so → cp 进 jniLibs → 直接调 gradlew
# （gradle 侧的 rustBuild 任务用 -x 跳过，避免它再去建链接）。
#
# 用法：
#   bash build-android.sh                 # 默认 arm64-v8a + release
#   bash build-android.sh x86_64 debug    # 模拟器调试包
#   bash build-android.sh arm64 release   # 出正式包
#
# 产物：CCL-<versionName>-<abi>.apk（仓库根目录，已用发布密钥签名）
set -euo pipefail

ABI="${1:-arm64}"
PROFILE="${2:-release}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 注意 jniLibs 目录名（arm64-v8a）和 gradle 输出目录名（arm64）不一样，
# 早期版本混用导致签名步骤找不到文件。
case "$ABI" in
  arm64)  RUST_TARGET=aarch64-linux-android; LIBS_DIR=arm64-v8a; OUT_ABI=arm64;  VARIANT=Arm64 ;;
  x86_64) RUST_TARGET=x86_64-linux-android;  LIBS_DIR=x86_64;    OUT_ABI=x86_64; VARIANT=X86_64 ;;
  *) echo "不支持的 ABI: $ABI（可选 arm64 / x86_64）" >&2; exit 1 ;;
esac

case "$PROFILE" in
  release) CARGO_FLAGS=--release; GRADLE_VARIANT="Release"; TYPE_DIR=release ;;
  debug)   CARGO_FLAGS=;          GRADLE_VARIANT="Debug";   TYPE_DIR=debug ;;
  *) echo "不支持的 profile: $PROFILE（可选 release / debug）" >&2; exit 1 ;;
esac

NDK="${NDK_HOME:-${ANDROID_HOME:-$HOME/AppData/Local/Android/Sdk}/ndk/28.2.13676358}"
NDK_BIN="$NDK/toolchains/llvm/prebuilt/windows-x86_64/bin"
export CC_aarch64_linux_android="$NDK_BIN/aarch64-linux-android24-clang.cmd"
export CXX_aarch64_linux_android="$NDK_BIN/aarch64-linux-android24-clang++.cmd"
export AR_aarch64_linux_android="$NDK_BIN/llvm-ar.exe"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
export CC_x86_64_linux_android="$NDK_BIN/x86_64-linux-android24-clang.cmd"
export CXX_x86_64_linux_android="$NDK_BIN/x86_64-linux-android24-clang++.cmd"
export AR_x86_64_linux_android="$NDK_BIN/llvm-ar.exe"
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$CC_x86_64_linux_android"

# 代理会让 gradle 的依赖解析走错路，安卓构建一律清掉
unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy

echo "==> [1/5] 前端构建"
(cd "$ROOT" && npm run build >/dev/null)

echo "==> [2/5] Rust 编译 ($RUST_TARGET, $PROFILE)"
(cd "$ROOT/src-tauri" && cargo build --target "$RUST_TARGET" $CARGO_FLAGS --lib)

echo "==> [3/5] 拷贝 .so 到 jniLibs"
SO="$ROOT/src-tauri/target/$RUST_TARGET/$PROFILE/libccl_lib.so"
mkdir -p "$ROOT/src-tauri/gen/android/app/src/main/jniLibs/$LIBS_DIR"
cp -f "$SO" "$ROOT/src-tauri/gen/android/app/src/main/jniLibs/$LIBS_DIR/libccl_lib.so"
ls -la "$ROOT/src-tauri/gen/android/app/src/main/jniLibs/$LIBS_DIR/libccl_lib.so"

echo "==> [4/5] Gradle 打包"
TASK="assemble${VARIANT}${GRADLE_VARIANT}"
(cd "$ROOT/src-tauri/gen/android" && ./gradlew "$TASK" -x "rustBuild${VARIANT}${GRADLE_VARIANT}" --console=plain -q)

OUT_DIR="$ROOT/src-tauri/gen/android/app/build/outputs/apk/$OUT_ABI/$TYPE_DIR"
UNSIGNED="$(ls "$OUT_DIR"/*.apk 2>/dev/null | head -1)"
if [ -z "$UNSIGNED" ]; then
  echo "没找到 gradle 产物（$OUT_DIR）" >&2
  exit 1
fi
echo "  产物 $UNSIGNED"

echo "==> [5/5] 签名"
VERSION="$(grep -m1 'tauri.android.versionName' "$ROOT/src-tauri/gen/android/app/tauri.properties" | cut -d= -f2)"
SIGNED="$ROOT/CCL-${VERSION}-${ABI}.apk"
BT="${ANDROID_HOME:-$HOME/AppData/Local/Android/Sdk}/build-tools/36.0.0"
"$BT/apksigner.bat" sign \
  --ks "$ROOT/keys/ccl-release.keystore" \
  --ks-pass pass:ccl123456 --key-pass pass:ccl123456 --ks-key-alias ccl \
  --out "$SIGNED" "$UNSIGNED"
echo "完成：$SIGNED"
