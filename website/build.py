#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
把截图压成 WebP 并内联进模板，产出单文件官网 index.html。
用法: python build.py
"""
import base64
import io
import os

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "assets")
TEMPLATE = os.path.join(HERE, "src", "index.template.html")
OUT = os.path.join(HERE, "index.html")

SHOTS = {
    "IMG_PCL": ("web_pcl2.png", 1800),
    "IMG_HMCL": ("web_hmcl.png", 1800),
    "IMG_VANILLA": ("web_vanilla.png", 1800),
    "IMG_DL": ("pcl_dl_final.png", 1500),
}


def encode(name, width):
    path = os.path.join(SRC, name)
    im = Image.open(path).convert("RGBA")
    if im.width > width:
        h = round(im.height * width / im.width)
        im = im.resize((width, h), Image.LANCZOS)
    buf = io.BytesIO()
    im.save(buf, "WEBP", quality=84, method=6)
    data = buf.getvalue()
    print(f"  {name:18s} {im.width}x{im.height}  {len(data)/1024:6.1f} KB webp")
    return "data:image/webp;base64," + base64.b64encode(data).decode()


def main():
    with open(TEMPLATE, "r", encoding="utf-8", newline="") as f:
        html = f.read()

    total = 0
    for key, (name, width) in SHOTS.items():
        uri = encode(name, width)
        total += len(uri)
        html = html.replace("__%s__" % key, uri)

    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        f.write(html)

    size = os.path.getsize(OUT) / 1024
    print(f"\n输出 {OUT}")
    print(f"单文件 {size:.0f} KB（图片内联 {total/1024:.0f} KB）")


if __name__ == "__main__":
    main()
