#!/usr/bin/env python3
"""Generate Seahelm app icon: sea-wave disc framed by a ship's-wheel (helm) ring."""

from PIL import Image, ImageDraw, ImageFont
import os
import json
import math

SIZE = 1024
# Seahelm palette (parent "sea" brand family)
BADGE_BG = (255, 255, 255, 255)  # light squircle field
NAVY = (10, 10, 100)             # #0A0A64 ring / base / outline
RED  = (255, 77, 46)             # #FF4D2E top sun band
CYAN = (25, 209, 224)            # #19D1E0 mid band
BLUE = (43, 111, 255)            # #2B6FFF lower wave


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


def _wave_fill(draw, cx, top, diameter, r, frac, amp, color):
    """Fill the disc region BELOW a sine crest whose baseline sits at
    `top + frac*diameter`. Painting bands top-colour first then each lower
    band as a below-fill yields clean stacked wave bands."""
    base_y = top + frac * diameter
    pts = []
    steps = 96
    for i in range(steps + 1):
        x = cx - r + (2 * r) * (i / steps)
        y = base_y - amp * math.sin(math.pi * (i / steps))
        pts.append((x, y))
    pts.append((cx + r, top + diameter * 2))
    pts.append((cx - r, top + diameter * 2))
    draw.polygon(pts, fill=color)


def _draw_at(size):
    img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    cx, cy = size / 2.0, size / 2.0
    s = size / 1024.0

    # Light rounded-square badge
    pad = int(72 * s)
    rounded_rect(draw, [pad, pad, size - pad, size - pad], int(180 * s), fill=BADGE_BG)

    ring_r = 330 * s          # outer radius of the navy ring band
    ring_w = max(int(34 * s), 4)
    disc_r = ring_r - ring_w - 18 * s

    # Helm spokes/handles, drawn under the ring so the ring caps them
    handle_len = 46 * s
    handle_w = max(int(22 * s), 4)
    knob = 17 * s
    for i in range(8):
        ang = math.pi / 8 + i * (math.pi / 4)
        x0 = cx + (ring_r - ring_w / 2) * math.cos(ang)
        y0 = cy + (ring_r - ring_w / 2) * math.sin(ang)
        x1 = cx + (ring_r + handle_len) * math.cos(ang)
        y1 = cy + (ring_r + handle_len) * math.sin(ang)
        draw.line([x0, y0, x1, y1], fill=NAVY, width=handle_w)
        draw.ellipse([x1 - knob, y1 - knob, x1 + knob, y1 + knob], fill=NAVY)

    # Navy ring band
    draw.ellipse([cx - ring_r, cy - ring_r, cx + ring_r, cy + ring_r],
                 outline=NAVY, width=ring_w)

    # Sea disc rendered on its own layer, then circular-masked
    disc = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    dd = ImageDraw.Draw(disc)
    top = cy - disc_r
    diameter = 2 * disc_r
    # Paint top band colour first, then each lower band as a below-fill.
    dd.ellipse([cx - disc_r, cy - disc_r, cx + disc_r, cy + disc_r], fill=RED)   # top sun band
    _wave_fill(dd, cx, top, diameter, disc_r, 0.34, 0.07 * diameter, CYAN)        # mid band
    _wave_fill(dd, cx, top, diameter, disc_r, 0.55, 0.07 * diameter, BLUE)        # lower wave
    _wave_fill(dd, cx, top, diameter, disc_r, 0.74, 0.06 * diameter, NAVY)        # deep base

    mask = Image.new('L', (size, size), 0)
    ImageDraw.Draw(mask).ellipse([cx - disc_r, cy - disc_r, cx + disc_r, cy + disc_r], fill=255)
    img.paste(disc, (0, 0), mask)
    return img


def draw_icon(size=1024):
    # Supersample for smooth circle/wave edges, then downscale.
    ss = 3
    big = _draw_at(size * ss)
    return big.resize((size, size), Image.LANCZOS)


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
