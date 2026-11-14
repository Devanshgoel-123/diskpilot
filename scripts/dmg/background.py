"""Draws the DMG window background at 1x and 2x.

Finder writes icon labels in black in light mode and white in dark mode, so
the background is a mid graphite that both read on (about 4:1 and 5:1).

    python3 scripts/dmg/background.py   # needs Pillow
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

W, H = 600, 380  # window content size, in points
APP_X, APPS_X, ICON_Y = 160, 440, 175  # icon centres, matching settings.py
BG = (110, 110, 118)
INK = (255, 255, 255)
OUT = Path(__file__).resolve().parent


def draw(scale: int) -> Image.Image:
    s = scale
    img = Image.new("RGB", (W * s, H * s), BG)
    d = ImageDraw.Draw(img, "RGBA")

    # A soft, lighter plate behind the two icons so they sit on something.
    d.rounded_rectangle(
        (60 * s, 80 * s, (W - 60) * s, 270 * s), radius=22 * s, fill=(255, 255, 255, 18)
    )

    # Arrow from the app to Applications: a short line and an open head.
    y = ICON_Y * s
    x0, x1 = (APP_X + 78) * s, (APPS_X - 78) * s
    ink = INK + (190,)
    d.line((x0, y, x1, y), fill=ink, width=4 * s)
    head = 13 * s
    d.line((x1 - head, y - head, x1, y), fill=ink, width=4 * s)
    d.line((x1 - head, y + head, x1, y), fill=ink, width=4 * s)
    for cx, cy in ((x0, y), (x1, y)):  # round the line caps
        r = 2 * s
        d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=ink)

    font = ImageFont.truetype("/System/Library/Fonts/SFNS.ttf", 15 * s)
    text = "Drag DiskPilot into Applications to install it"
    tw = d.textlength(text, font=font)
    d.text(((W * s - tw) / 2, 312 * s), text, font=font, fill=INK + (235,))
    return img


if __name__ == "__main__":
    draw(1).save(OUT / "background.png")
    draw(2).save(OUT / "background@2x.png")
    print("wrote", OUT / "background.png", "and @2x")
