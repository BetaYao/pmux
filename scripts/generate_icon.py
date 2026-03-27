#!/usr/bin/env python3
"""Generate amux app icon: stacked terminal cards, cyan + deep blue."""

from PIL import Image, ImageDraw, ImageFont
import os
import json

SIZE = 1024
# Colors
BG = (15, 23, 42)        # #0f172a deep blue
STROKE = (34, 211, 238)  # #22d3ee cyan
PANE_FILL = (22, 78, 99) # #164e63 dark teal
PANE_STROKE = (34, 211, 238)

def rounded_rect(draw, xy, radius, fill=None, outline=None, width=1):
    """Draw a rounded rectangle."""
    x0, y0, x1, y1 = xy
    r = radius
    # Fill
    if fill:
        draw.rectangle([x0 + r, y0, x1 - r, y1], fill=fill)
        draw.rectangle([x0, y0 + r, x1, y1 - r], fill=fill)
        draw.pieslice([x0, y0, x0 + 2*r, y0 + 2*r], 180, 270, fill=fill)
        draw.pieslice([x1 - 2*r, y0, x1, y0 + 2*r], 270, 360, fill=fill)
        draw.pieslice([x0, y1 - 2*r, x0 + 2*r, y1], 90, 180, fill=fill)
        draw.pieslice([x1 - 2*r, y1 - 2*r, x1, y1], 0, 90, fill=fill)
    # Outline
    if outline:
        draw.arc([x0, y0, x0 + 2*r, y0 + 2*r], 180, 270, fill=outline, width=width)
        draw.arc([x1 - 2*r, y0, x1, y0 + 2*r], 270, 360, fill=outline, width=width)
        draw.arc([x0, y1 - 2*r, x0 + 2*r, y1], 90, 180, fill=outline, width=width)
        draw.arc([x1 - 2*r, y1 - 2*r, x1, y1], 0, 90, fill=outline, width=width)
        draw.line([x0 + r, y0, x1 - r, y0], fill=outline, width=width)
        draw.line([x0 + r, y1, x1 - r, y1], fill=outline, width=width)
        draw.line([x0, y0 + r, x0, y1 - r], fill=outline, width=width)
        draw.line([x1, y0 + r, x1, y1 - r], fill=outline, width=width)


def draw_icon(size=1024):
    img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    s = size / 140  # scale factor from our 140x140 design

    # Outer squircle
    pad = int(10 * s)
    outer_r = int(26 * s)
    outer_w = max(int(2.5 * s), 3)
    rounded_rect(draw, [pad, pad, size - pad, size - pad], outer_r, fill=BG, outline=STROKE, width=outer_w)

    # Pane dimensions
    pane_gap = int(8 * s)
    pane_pad = int(24 * s)
    pane_r = int(6 * s)
    pane_w = max(int(1.5 * s), 2)

    total_inner = size - 2 * pane_pad - pane_gap
    pane_size = total_inner // 2

    positions = [
        (pane_pad, pane_pad),
        (pane_pad + pane_size + pane_gap, pane_pad),
        (pane_pad, pane_pad + pane_size + pane_gap),
        (pane_pad + pane_size + pane_gap, pane_pad + pane_size + pane_gap),
    ]

    # Try to find a monospace font
    font = None
    font_size = int(13 * s)
    font_paths = [
        "/System/Library/Fonts/SFMono-Regular.otf",
        "/System/Library/Fonts/Menlo.ttc",
        "/System/Library/Fonts/Monaco.dfont",
        "/Library/Fonts/SF-Mono-Regular.otf",
    ]
    for fp in font_paths:
        if os.path.exists(fp):
            try:
                font = ImageFont.truetype(fp, font_size)
                break
            except Exception:
                continue
    if font is None:
        font = ImageFont.load_default()

    prompt = "\u276f_"  # ❯_

    for (px, py) in positions:
        x0, y0 = px, py
        x1, y1 = px + pane_size, py + pane_size
        rounded_rect(draw, [x0, y0, x1, y1], pane_r, fill=PANE_FILL, outline=PANE_STROKE, width=pane_w)

        # Draw prompt text centered vertically, left-padded
        text_x = x0 + int(8 * s)
        text_y = y0 + pane_size // 2 - font_size // 2
        draw.text((text_x, text_y), prompt, fill=STROKE, font=font)

    return img


def main():
    project_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    asset_dir = os.path.join(project_dir, "Assets.xcassets", "AppIcon.appiconset")
    os.makedirs(asset_dir, exist_ok=True)

    # Generate 1024x1024 master icon
    icon = draw_icon(1024)
    icon_path = os.path.join(asset_dir, "icon_1024x1024.png")
    icon.save(icon_path, "PNG")
    print(f"Saved {icon_path}")

    # Generate additional sizes
    sizes = [512, 256, 128, 64, 32, 16]
    for sz in sizes:
        resized = icon.resize((sz, sz), Image.LANCZOS)
        path = os.path.join(asset_dir, f"icon_{sz}x{sz}.png")
        resized.save(path, "PNG")
        print(f"Saved {path}")

    # Write Contents.json
    contents = {
        "images": [
            {
                "filename": "icon_16x16.png",
                "idiom": "mac",
                "scale": "1x",
                "size": "16x16"
            },
            {
                "filename": "icon_32x32.png",
                "idiom": "mac",
                "scale": "2x",
                "size": "16x16"
            },
            {
                "filename": "icon_32x32.png",
                "idiom": "mac",
                "scale": "1x",
                "size": "32x32"
            },
            {
                "filename": "icon_64x64.png",
                "idiom": "mac",
                "scale": "2x",
                "size": "32x32"
            },
            {
                "filename": "icon_128x128.png",
                "idiom": "mac",
                "scale": "1x",
                "size": "128x128"
            },
            {
                "filename": "icon_256x256.png",
                "idiom": "mac",
                "scale": "2x",
                "size": "128x128"
            },
            {
                "filename": "icon_256x256.png",
                "idiom": "mac",
                "scale": "1x",
                "size": "256x256"
            },
            {
                "filename": "icon_512x512.png",
                "idiom": "mac",
                "scale": "2x",
                "size": "256x256"
            },
            {
                "filename": "icon_512x512.png",
                "idiom": "mac",
                "scale": "1x",
                "size": "512x512"
            },
            {
                "filename": "icon_1024x1024.png",
                "idiom": "mac",
                "scale": "2x",
                "size": "512x512"
            }
        ],
        "info": {
            "author": "xcode",
            "version": 1
        }
    }

    contents_path = os.path.join(asset_dir, "Contents.json")
    with open(contents_path, "w") as f:
        json.dump(contents, f, indent=2)
    print(f"Saved {contents_path}")

    # Write Assets.xcassets/Contents.json if missing
    xcassets_dir = os.path.join(project_dir, "Assets.xcassets")
    xcassets_contents = os.path.join(xcassets_dir, "Contents.json")
    if not os.path.exists(xcassets_contents):
        with open(xcassets_contents, "w") as f:
            json.dump({"info": {"author": "xcode", "version": 1}}, f, indent=2)
        print(f"Saved {xcassets_contents}")


if __name__ == "__main__":
    main()
