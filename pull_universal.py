"""把 JRE universal 包拉全（单连接续传，多镜像轮换）。

弱网下唯一靠谱的做法：一条连接 + Range 续传，断了就换下一个镜像接着写。
多连接反而更慢（加速节点按连接限速，实测单连接 122 KB/s、同节点 8 连接合计 10 KB/s）。
"""
import hashlib
import os
import sys
import time
import urllib.request

GH = "https://github.com/SouhardyaMaybe/EclipseJRE/releases/download/jres-v1.0.0"
MIRRORS = [
    "https://axisnow.gh-proxy.org/",
    "https://gh-proxy.com/",
    "https://ghproxy.net/",
]
NAME = "jre17-universal.tar.xz"
SHA = "e84d33561ebd19c004c31d3e04a1ea9af95d906cd6fda6778a63f8ed218dffb2"
SIZE = 22008312


def log(msg):
    print(time.strftime("%H:%M:%S"), msg, flush=True)


def main(outdir):
    os.makedirs(outdir, exist_ok=True)
    dest = os.path.join(outdir, NAME)
    deadline = time.time() + float(os.environ.get("MAX_HOURS", "3")) * 3600

    while time.time() < deadline:
        have = os.path.getsize(dest) if os.path.exists(dest) else 0
        if have >= SIZE:
            break
        for m in MIRRORS:
            have = os.path.getsize(dest) if os.path.exists(dest) else 0
            if have >= SIZE:
                break
            url = m + GH + "/" + NAME
            req = urllib.request.Request(
                url, headers={"Range": f"bytes={have}-", "User-Agent": "ccl"})
            try:
                with urllib.request.urlopen(req, timeout=30) as r:
                    if have and r.status != 206:
                        raise RuntimeError(f"HTTP {r.status}（不支持续传）")
                    with open(dest, "ab") as f:
                        last = time.time()
                        while True:
                            b = r.read(65536)
                            if not b:
                                break
                            f.write(b)
                            if time.time() - last > 30:
                                last = time.time()
                                got = os.path.getsize(dest)
                                log(f"{m.split('/')[2]} {got/1048576:.2f}/{SIZE/1048576:.2f} MB")
            except Exception as e:
                log(f"{m.split('/')[2]} 中断: {e}（已 {os.path.getsize(dest) if os.path.exists(dest) else 0} 字节）")
        time.sleep(2)

    if not os.path.exists(dest):
        log("没拿到任何数据")
        return 1
    have = os.path.getsize(dest)
    h = hashlib.sha256()
    with open(dest, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    ok = have == SIZE and h.hexdigest() == SHA
    log(f"完成 size={have} sha={h.hexdigest()[:16]}… {'OK' if ok else 'MISMATCH'}")
    return 0 if ok else 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "F:/Projects/CCL/jre_cache"))
