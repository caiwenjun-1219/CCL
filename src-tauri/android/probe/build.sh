#!/usr/bin/env bash
# 编译安卓侧要进内嵌 JVM 的两个小 jar，产物直接落进 APK assets。
#
#   1. probe.jar  —— M2 的 LWJGL 连通性探针（ccl.lwjgl.Probe）
#   2. shim.jar   —— org.lwjgl.glfw.CallbackBridge 的兼容实现（见该类注释）
#
# 为什么预编译而不是构建期编译：两个加起来不到 10 KB，编译一次就够；
# 放进 assets 后运行时随 natives 一起解包，不需要给 Gradle / Tauri 加 Java 编译任务。
#
# 用法：bash src-tauri/android/probe/build.sh
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ASSETS="$HERE/../../plugins/ccl-android/android/src/main/assets/lwjgl"
JDK="${JAVA_HOME:-C:/Program Files/Microsoft/jdk-21.0.9.10-hotspot}"
JAVAC="$JDK/bin/javac"
JAR="$JDK/bin/jar"

# --release 8：设备上可能是 JRE 8 / 17 / 21，8 的字节码全都能吃
build() {
  local name="$1" srcdir="$2"
  local tmp
  tmp="$(mktemp -d)"
  # 编译失败必须直接失败：之前 javac 报错被吞掉，只留下一个 314 字节的空 jar，
  # 设备上跑的是「类不存在」，排查方向全错。
  "$JAVAC" --release 8 -encoding UTF-8 -nowarn -d "$tmp" $(find "$srcdir" -name '*.java') ||
    { echo "javac 失败：$srcdir" >&2; rm -rf "$tmp"; exit 1; }
  "$JAR" --create --file "$ASSETS/$name" -C "$tmp" .
  rm -rf "$tmp"
  printf '%-12s %s 字节\n' "$name" "$(stat -c%s "$ASSETS/$name")"
}

build probe.jar "$HERE"
build shim.jar "$HERE/../shim"
