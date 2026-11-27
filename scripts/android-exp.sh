#!/usr/bin/env bash
# 真机实验台：把「改 JVM 参数 → 重启跑 → 收日志 → 判存活」这套动作固化成一条命令。
#
# 为什么要脚本：这些实验每次都是同样的五步，手敲容易漏（比如忘了 force-stop
# 就 am start，跑的还是上一个进程；或者忘了 logcat -c，结论被旧日志污染）。
#
# 用法：
#   bash scripts/android-exp.sh status            # 设备/开关/已安装版本
#   bash scripts/android-exp.sh build-tc          # 编译 T.class 并推到设备
#   bash scripts/android-exp.sh jvm "-Xint" probe 120
#   bash scripts/android-exp.sh xint              # 等价：jvm "-Xint" probe 120
#   bash scripts/android-exp.sh gc Serial         # -XX:+UseSerialGC
#   bash scripts/android-exp.sh apc 8             # -XX:ActiveProcessorCount=8
#   bash scripts/android-exp.sh heap 180          # auto-embed + 堆压力（验 GC 假设）
#   bash scripts/android-exp.sh fcl               # 找 FCL 的 JRE
#   bash scripts/android-exp.sh fclswap           # 把 FCL 的 jre21 拷来替换我们的
#
# 判定口径统一写在每次运行末尾：进程是否存活 + crash buffer 里有没有 Abort message。

set -uo pipefail

PKG="com.caiwenjun.ccl"
ACT="$PKG/.MainActivity"
DATA="/data/data/$PKG/CCL"
LOG_TAG="RustStdoutStderr"
ADB="adb -s ${ANDROID_SERIAL:-10CE1K14E0001LS}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$ROOT/.tmp"

# MSYS 会把 /data/... 当成 Windows 路径改写，凡是传给设备的绝对路径都要关掉
dev() { MSYS_NO_PATHCONV=1 $ADB shell "$@"; }

# app 自己的 uid 才能写 DATA；/data/local/tmp 对 app 不可读（0711），所以用管道
push_data() {
    local local_file="$1" remote="$2"
    MSYS_NO_PATHCONV=1 $ADB shell "run-as $PKG sh -c 'cat > $remote'" < "$local_file"
}

set_flag() {
    dev "run-as $PKG sh -c 'cd $DATA && rm -f auto-launch auto-plan auto-probe auto-embed auto-touch && touch $1'" >/dev/null
    echo "  模式开关：$1"
}

write_opts() {
    local text="$1"
    printf '%s\n' "$text" > "$TMP/jvm-opts.txt"
    push_data "$TMP/jvm-opts.txt" "$DATA/java/jvm-opts.txt"
    echo "  jvm-opts：$(echo "$text" | tr '\n' ' ')"
}

# 跑一次：清日志 → 重启 → 等 N 秒 → 报告
run_once() {
    local mode="$1" secs="$2"
    set_flag "$mode"
    dev "am force-stop $PKG" >/dev/null
    $ADB logcat -c >/dev/null 2>&1
    $ADB logcat -b crash -c >/dev/null 2>&1
    dev "am start -n $ACT" >/dev/null 2>&1
    echo "  启动完成，观察 ${secs}s …"
    sleep "$secs"

    local pid abort
    pid="$(dev "pidof $PKG" 2>/dev/null | tr -d '\r')"
    abort="$($ADB logcat -d -b crash 2>/dev/null | grep -m1 'Abort message' | tr -d '\r')"

    echo
    echo "──────── 判定 ────────"
    if [ -n "$pid" ]; then
        echo "进程存活 pid=$pid（${secs}s 内没崩）"
    else
        echo "进程已消失"
    fi
    if [ -n "$abort" ]; then
        echo "崩溃：$abort"
    else
        echo "crash buffer 干净"
    fi
    echo "──────── 关键日志 ────────"
    $ADB logcat -d 2>/dev/null | grep -E "$LOG_TAG|CCLInput|debug-jvm|ccl-lwjgl|ccl-touch" | tail -14
}

build_tc() {
    local jdk="${JAVA_HOME:-C:/Program Files/Microsoft/jdk-21.0.9.10-hotspot}"
    local out="$TMP/tc"
    rm -rf "$out"; mkdir -p "$out"
    # --release 8：设备上可能是 JRE 8/17/21，8 的字节码全都吃
    "$jdk/bin/javac" --release 8 -encoding UTF-8 -nowarn -d "$out" "$ROOT/src-tauri/android/tc/T.java" \
        || { echo "javac 失败"; return 1; }
    ls -la "$out"
    dev "run-as $PKG mkdir -p $DATA/tc" >/dev/null
    push_data "$out/T.class" "$DATA/tc/T.class"
    echo "  已推送 T.class（$(dev "run-as $PKG wc -c < $DATA/tc/T.class" | tr -d '\r') 字节）"
}

cmd="${1:-status}"

case "$cmd" in
status)
    echo "== 设备 =="
    $ADB devices | tail -n +2
    echo "== 应用 =="
    dev "dumpsys package $PKG | grep -E 'versionName|lastUpdateTime'" 2>/dev/null | tr -d '\r'
    echo "== 开关 =="
    dev "run-as $PKG sh -c 'cd $DATA && ls -la auto-* 2>/dev/null; echo ---; cat java/jvm-opts.txt 2>/dev/null'" | tr -d '\r'
    echo "== JRE =="
    dev "run-as $PKG sh -c 'ls -d $DATA/java/jre*'" | tr -d '\r'
    ;;
build-tc) build_tc ;;
jvm)
    write_opts "${2:-}"
    run_once "${3:-probe}" "${4:-120}"
    ;;
xint)  write_opts "-Xint"; run_once probe 120 ;;
gc)
    write_opts "-XX:+Use${2:-Serial}GC"
    run_once probe 120
    ;;
apc)   write_opts "-XX:ActiveProcessorCount=${2:-8}"; run_once probe 120 ;;
heap)
    build_tc || exit 1
    write_opts "-Dccl.heapTest=1"
    # auto-embed 用 -Xmx256m，堆压力下几十秒就能触发多轮 GC
    run_once embed "${2:-180}"
    ;;
fcl)
    echo "== FCL 包与数据目录 =="
    dev "pm list packages | grep -i tungsten" | tr -d '\r'
    dev "ls -d /sdcard/Android/data/com.tungsten.fcl/files/* 2>/dev/null" | tr -d '\r'
    echo "== 找 jre =="
    dev "find /sdcard -maxdepth 7 -type d -name 'jre2*' 2>/dev/null | head -10" | tr -d '\r'
    ;;
fclswap)
    SRC="$2"
    if [ -z "${SRC:-}" ]; then
        echo "用法：android-exp.sh fclswap /sdcard/Android/data/com.tungsten.fcl/files/runtime/java/jre21"
        echo "（先跑 fcl 子命令找路径）"
        exit 1
    fi
    echo "注意：会先备份现有 CCL/java/jre21-arm64 为 jre21-arm64.bak"
    dev "run-as $PKG sh -c 'cd $DATA/java && [ -d jre21-arm64 ] && mv jre21-arm64 jre21-arm64.bak; mkdir -p jre21-arm64; true'"
    dev "cp -r $SRC/. /sdcard/CCL-jre21-tmp/ 2>/dev/null; true"
    # app 读不了 /sdcard 时不走这条；提示手工用 tar
    echo "若上面没成功，改用：adb shell run-as $PKG tar -xf /sdcard/fcl-jre21.tar -C $DATA/java/jre21-arm64"
    ;;
*)
    echo "未知子命令：$cmd"
    sed -n '10,25p' "$0"
    exit 1
    ;;
esac
