"""Renders every app icon from assets/icon.svg, with a transparent background.

macOS sizes put the tile on Apple's icon grid (an 824 px body in 1024, with
a soft shadow), so DiskPilot is the same size as other apps in the Dock and
Finder. The Windows .ico uses the tile edge to edge, as Windows icons do.

    pip install resvg-py pillow && python3 scripts/render_icons.py
"""

import io
import subprocess
import tempfile
from pathlib import Path

import resvg_py
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "src-tauri" / "icons"
TILE = (64, 64, 960, 960)  # the rounded tile in icon.svg


def tile() -> Image.Image:
    png = resvg_py.svg_to_bytes(svg_path=str(ROOT / "assets" / "icon.svg"), width=1024, height=1024)
    return Image.open(io.BytesIO(bytes(png))).convert("RGBA").crop(TILE)


def mac_icon(t: Image.Image) -> Image.Image:
    body = t.resize((824, 824), Image.LANCZOS)
    out = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    shadow = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    shadow.paste((0, 0, 0, 255), (100, 112), body.getchannel("A").point(lambda v: int(v * 0.30)))
    out.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(14)))
    out.alpha_composite(body, (100, 100))
    return out


def sized(img: Image.Image, px: int) -> Image.Image:
    return img.resize((px, px), Image.LANCZOS)


def main() -> None:
    t = tile()
    mac = mac_icon(t)
    for name, px in (("32x32.png", 32), ("64x64.png", 64), ("128x128.png", 128),
                     ("128x128@2x.png", 256), ("icon.png", 512)):
        sized(mac, px).save(ICONS / name)

    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "icon.iconset"
        iconset.mkdir()
        for pt in (16, 32, 128, 256, 512):
            sized(mac, pt).save(iconset / f"icon_{pt}x{pt}.png")
            sized(mac, pt * 2).save(iconset / f"icon_{pt}x{pt}@2x.png")
        subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(ICONS / "icon.icns")], check=True)

    win = sized(t, 256)
    win.save(ICONS / "icon.ico", sizes=[(s, s) for s in (16, 24, 32, 48, 64, 256)])
    print("rendered icons into", ICONS)


if __name__ == "__main__":
    main()
