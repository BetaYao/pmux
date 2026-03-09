#!/usr/bin/env python3
"""
Generate macOS app icon from the pudding logo.
1. Crop the image to remove white space
2. Generate all required sizes for .icns file
3. Supports --dev flag for dev mode variant (adds DEV badge)
"""

from PIL import Image, ImageDraw, ImageFont
import os
import subprocess
import argparse

# macOS app icon sizes
ICON_SIZES = [
    (16, 16),
    (32, 32),
    (64, 64),
    (128, 128),
    (256, 256),
    (512, 512),
    (1024, 1024),  # @2x versions included
]

def remove_white_background(image, threshold=240):
    """Replace white/near-white pixels with transparent."""
    img = image.convert('RGBA')
    data = img.getdata()
    new_data = []
    for r, g, b, a in data:
        if r > threshold and g > threshold and b > threshold:
            new_data.append((r, g, b, 0))
        else:
            new_data.append((r, g, b, a))
    img.putdata(new_data)
    return img


def crop_to_content(image, threshold=240):
    """Crop image to remove white/light borders."""
    rgb_image = image.convert('RGB')
    width, height = rgb_image.size

    def is_background(pixel):
        return all(c > threshold for c in pixel[:3])

    top = 0
    for y in range(height):
        if not all(is_background(rgb_image.getpixel((x, y))) for x in range(width)):
            top = y
            break

    bottom = height - 1
    for y in range(height - 1, -1, -1):
        if not all(is_background(rgb_image.getpixel((x, y))) for x in range(width)):
            bottom = y
            break

    left = 0
    for x in range(width):
        if not all(is_background(rgb_image.getpixel((x, y))) for y in range(height)):
            left = x
            break

    right = width - 1
    for x in range(width - 1, -1, -1):
        if not all(is_background(rgb_image.getpixel((x, y))) for y in range(height)):
            right = x
            break

    padding = int(min(width, height) * 0.08)
    crop_box = (
        max(0, left - padding),
        max(0, top - padding),
        min(width, right + padding + 1),
        min(height, bottom + padding + 1),
    )
    return image.crop(crop_box)


def create_square_image(image, bg_color=(0, 0, 0, 0)):
    """Create a square image: transparent background with pudding centered."""
    width, height = image.size
    size = max(width, height)

    # Transparent background — macOS adds its own rounded-rect background
    square = Image.new('RGBA', (size, size), bg_color)

    # Remove white from pudding so it composites cleanly
    logo = remove_white_background(image)

    x = (size - width) // 2
    y = (size - height) // 2
    square.paste(logo, (x, y), logo)

    return square

def add_dev_badge(image):
    """Add a small DEV badge to the bottom-right corner of the image."""
    img = image.convert('RGBA')
    size = img.size[0]
    draw = ImageDraw.Draw(img)

    # Badge dimensions: ~18% of icon size, positioned in corner with 4% margin
    badge_height = max(14, int(size * 0.18))
    badge_width = int(badge_height * 1.8)
    margin = max(2, int(size * 0.04))
    x1 = size - badge_width - margin
    y1 = size - badge_height - margin
    x2 = size - margin
    y2 = size - margin

    # Rounded rectangle for badge (amber/orange dev indicator)
    radius = badge_height // 4
    draw.rounded_rectangle(
        [(x1, y1), (x2, y2)],
        radius=radius,
        fill=(255, 149, 0, 230),  # Amber/orange
        outline=(200, 100, 0, 255),
        width=max(1, size // 256),
    )

    # DEV text - use default font, scaled to badge
    font_size = max(8, badge_height // 2)
    font_paths = [
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/SFNSMono.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    ]
    font = None
    for path in font_paths:
        if os.path.exists(path):
            try:
                font = ImageFont.truetype(path, font_size)
                break
            except (OSError, IOError):
                continue
    if font is None:
        font = ImageFont.load_default()

    text = "DEV"
    bbox = draw.textbbox((0, 0), text, font=font)
    text_width = bbox[2] - bbox[0]
    text_height = bbox[3] - bbox[1]
    text_x = x1 + (badge_width - text_width) // 2
    text_y = y1 + (badge_height - text_height) // 2 - 1
    draw.text((text_x, text_y), text, fill=(255, 255, 255, 255), font=font)

    return img

def generate_iconset(source_path, output_dir, dev_mode=False):
    """Generate .iconset folder with all required sizes."""
    # Load and process source image
    img = Image.open(source_path)
    print(f"Original size: {img.size}")

    # Crop to content
    cropped = crop_to_content(img)
    print(f"Cropped size: {cropped.size}")

    # Make it square
    square = create_square_image(cropped)
    print(f"Square size: {square.size}")

    if dev_mode:
        square = add_dev_badge(square)
        print("Added DEV badge")

    # Create iconset directory
    iconset_name = "pmux.iconset"
    iconset_path = os.path.join(output_dir, iconset_name)
    os.makedirs(iconset_path, exist_ok=True)

    # Generate all sizes
    for size in ICON_SIZES:
        # Regular size
        resized = square.resize(size, Image.Resampling.LANCZOS)
        filename = f"icon_{size[0]}x{size[1]}.png"
        resized.save(os.path.join(iconset_path, filename), 'PNG')
        print(f"Generated {filename}")

        # @2x version (for Retina displays)
        if size[0] <= 512:
            retina_size = (size[0] * 2, size[1] * 2)
            retina = square.resize(retina_size, Image.Resampling.LANCZOS)
            retina_filename = f"icon_{size[0]}x{size[1]}@2x.png"
            retina.save(os.path.join(iconset_path, retina_filename), 'PNG')
            print(f"Generated {retina_filename}")

    return iconset_path

def convert_to_icns(iconset_path, output_path):
    """Convert .iconset to .icns using iconutil."""
    result = subprocess.run(
        ['iconutil', '-c', 'icns', iconset_path, '-o', output_path],
        capture_output=True,
        text=True
    )

    if result.returncode != 0:
        print(f"Error converting to icns: {result.stderr}")
        return False

    print(f"Successfully created {output_path}")
    return True

def main():
    parser = argparse.ArgumentParser(description="Generate pmux app icon from pudding logo")
    parser.add_argument(
        "--dev",
        action="store_true",
        help="Generate dev mode variant with DEV badge in corner",
    )
    args = parser.parse_args()

    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.dirname(script_dir)

    source_image = os.path.join(project_root, "布丁logo_极简线条.png")
    resources_dir = script_dir

    mode_str = " (dev mode)" if args.dev else ""
    print(f"Processing {source_image}{mode_str}...")

    # Generate iconset
    iconset_path = generate_iconset(source_image, resources_dir, dev_mode=args.dev)

    # Convert to icns
    icns_path = os.path.join(resources_dir, "pmux.icns")
    if convert_to_icns(iconset_path, icns_path):
        # Clean up iconset folder
        import shutil
        shutil.rmtree(iconset_path)
        print(f"Cleaned up temporary files")

    print("\nDone! Icon saved to:", icns_path)

if __name__ == "__main__":
    main()
