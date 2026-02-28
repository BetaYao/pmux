#!/usr/bin/env python3
"""
截图图像分析工具
用于自动化验证 sidebar 状态、光标位置和颜色显示
"""

import sys
from PIL import Image
import subprocess

def get_pixel_color(image_path, x, y):
    """获取指定坐标的像素颜色"""
    try:
        img = Image.open(image_path)
        rgb = img.getpixel((x, y))
        return rgb
    except Exception as e:
        print(f"Error reading pixel: {e}", file=sys.stderr)
        return None

def analyze_region(image_path, x, y, width, height):
    """分析指定区域的平均颜色"""
    try:
        img = Image.open(image_path)
        region = img.crop((x, y, x + width, y + height))
        
        # 计算平均颜色
        pixels = list(region.getdata())
        r_sum = sum(p[0] for p in pixels)
        g_sum = sum(p[1] for p in pixels)
        b_sum = sum(p[2] for p in pixels)
        count = len(pixels)
        
        avg_color = (r_sum // count, g_sum // count, b_sum // count)
        
        # 计算颜色方差（判断是否是纯色）
        r_var = sum((p[0] - avg_color[0]) ** 2 for p in pixels) / count
        g_var = sum((p[1] - avg_color[1]) ** 2 for p in pixels) / count
        b_var = sum((p[2] - avg_color[2]) ** 2 for p in pixels) / count
        variance = (r_var + g_var + b_var) / 3
        
        return {
            'avg_color': avg_color,
            'variance': variance,
            'pixel_count': count
        }
    except Exception as e:
        print(f"Error analyzing region: {e}", file=sys.stderr)
        return None

def detect_cursor(image_path, terminal_region):
    """
    检测 terminal 光标位置
    terminal_region: (x, y, width, height)
    通过寻找高亮像素块来检测光标
    """
    try:
        img = Image.open(image_path)
        x, y, w, h = terminal_region
        region = img.crop((x, y, x + w, y + h))
        pixels = region.load()
        
        # 查找可能的 cursor（通常是高对比度、小区域）
        cursor_candidates = []
        
        for py in range(h - 20):  # 扫描行，假设光标高度约 20px
            for px in range(w - 8):  # 扫描列，假设光标宽度约 8px
                # 检查 8x20 区域是否是高亮块
                is_highlight = True
                brightness_sum = 0
                
                for dy in range(20):
                    for dx in range(8):
                        r, g, b = pixels[px + dx, py + dy]
                        brightness = (r + g + b) / 3
                        brightness_sum += brightness
                        
                        # 光标通常是亮色（蓝色或白色）
                        if brightness < 100:  # 太暗，不太可能是光标
                            is_highlight = False
                            break
                    if not is_highlight:
                        break
                
                if is_highlight:
                    avg_brightness = brightness_sum / 160
                    cursor_candidates.append({
                        'x': px,
                        'y': py,
                        'brightness': avg_brightness
                    })
        
        # 返回最亮的候选
        if cursor_candidates:
            cursor_candidates.sort(key=lambda c: c['brightness'], reverse=True)
            return cursor_candidates[0]
        
        return None
    except Exception as e:
        print(f"Error detecting cursor: {e}", file=sys.stderr)
        return None

def check_sidebar_status(image_path, sidebar_region):
    """
    检测 sidebar 状态颜色
    返回: 'running', 'error', 'input', 'idle', 或 'unknown'
    """
    try:
        img = Image.open(image_path)
        x, y, w, h = sidebar_region
        region = img.crop((x, y, x + w, y + h))
        
        # 采样多个点来获取主要颜色
        pixels = list(region.getdata())
        
        # 颜色阈值（根据 pmux 的设计调整）
        status_colors = {
            'running': {'r': (50, 150), 'g': (150, 255), 'b': (50, 150)},  # 绿色
            'error': {'r': (150, 255), 'g': (50, 100), 'b': (50, 100)},    # 红色
            'input': {'r': (50, 150), 'g': (100, 200), 'b': (150, 255)},   # 蓝色
            'idle': {'r': (100, 200), 'g': (100, 200), 'b': (100, 200)},   # 灰色
        }
        
        # 计算主要颜色占比
        color_counts = {key: 0 for key in status_colors.keys()}
        
        for r, g, b in pixels:
            for status, ranges in status_colors.items():
                if (ranges['r'][0] <= r <= ranges['r'][1] and
                    ranges['g'][0] <= g <= ranges['g'][1] and
                    ranges['b'][0] <= b <= ranges['b'][1]):
                    color_counts[status] += 1
                    break
        
        # 返回占比最高的状态
        total = sum(color_counts.values())
        if total > 0:
            max_status = max(color_counts.keys(), key=lambda k: color_counts[k])
            confidence = color_counts[max_status] / total
            return {'status': max_status, 'confidence': confidence}
        
        return {'status': 'unknown', 'confidence': 0}
    except Exception as e:
        print(f"Error checking sidebar: {e}", file=sys.stderr)
        return {'status': 'error', 'confidence': 0}

def verify_window(image_path, min_width=400, min_height=300):
    """
    验证 pmux 窗口是否正常显示
    - 图片尺寸合理（非 0 或过小）
    - 主色调为深色（pmux 主题 rgb 0x1e1e1e 附近）
    - 有内容（颜色方差 > 0，非纯白/纯黑/透明）
    返回: {'ok': bool, 'reason': str, 'avg_brightness': float, 'variance': float, 'width': int, 'height': int}
    """
    try:
        img = Image.open(image_path)
        w, h = img.size

        if w < min_width or h < min_height:
            return {
                'ok': False,
                'reason': f'window_too_small',
                'avg_brightness': 0,
                'variance': 0,
                'width': w,
                'height': h,
            }

        pixels = list(img.getdata())
        count = len(pixels)
        r_sum = sum(p[0] for p in pixels)
        g_sum = sum(p[1] for p in pixels)
        b_sum = sum(p[2] for p in pixels)
        avg_r = r_sum / count
        avg_g = g_sum / count
        avg_b = b_sum / count
        avg_brightness = (avg_r + avg_g + avg_b) / 3

        r_var = sum((p[0] - avg_r) ** 2 for p in pixels) / count
        g_var = sum((p[1] - avg_g) ** 2 for p in pixels) / count
        b_var = sum((p[2] - avg_b) ** 2 for p in pixels) / count
        variance = (r_var + g_var + b_var) / 3

        # pmux 深色主题：平均亮度应 < 120（0x1e1e1e ≈ 30）
        # 纯白/透明会接近 255，纯黑会接近 0 但方差极低
        if avg_brightness > 200:
            return {
                'ok': False,
                'reason': 'window_too_bright',
                'avg_brightness': avg_brightness,
                'variance': variance,
                'width': w,
                'height': h,
            }

        # 有内容的窗口应有一定颜色变化（sidebar、terminal、文字等）
        if variance < 50:
            return {
                'ok': False,
                'reason': 'window_too_flat',
                'avg_brightness': avg_brightness,
                'variance': variance,
                'width': w,
                'height': h,
            }

        return {
            'ok': True,
            'reason': 'ok',
            'avg_brightness': avg_brightness,
            'variance': variance,
            'width': w,
            'height': h,
        }
    except Exception as e:
        return {
            'ok': False,
            'reason': f'error:{e}',
            'avg_brightness': 0,
            'variance': 0,
            'width': 0,
            'height': 0,
        }


def has_multiple_colors(image_path, region, min_colors=3):
    """
    检查区域是否包含多种颜色（用于验证颜色显示）
    """
    try:
        img = Image.open(image_path)
        x, y, w, h = region
        region_img = img.crop((x, y, x + w, y + h))
        pixels = list(region_img.getdata())
        
        # 简化颜色（降低分辨率）
        simplified = set()
        for r, g, b in pixels:
            # 将颜色分到 32 个 bucket
            sr = r // 32
            sg = g // 32
            sb = b // 32
            simplified.add((sr, sg, sb))
        
        color_count = len(simplified)
        return {
            'has_multiple_colors': color_count >= min_colors,
            'color_count': color_count,
            'simplified_palette': list(simplified)[:10]  # 前10个颜色样本
        }
    except Exception as e:
        print(f"Error checking colors: {e}", file=sys.stderr)
        return {'has_multiple_colors': False, 'color_count': 0}

if __name__ == '__main__':
    if len(sys.argv) < 3:
        print("Usage: python3 image_analysis.py <command> <image_path> [args...]")
        print("")
        print("Commands:")
        print("  verify_window <image> [min_w] [min_h]   - Verify pmux window is visible/normal")
        print("  sidebar_status <image> <x> <y> <w> <h>  - Detect sidebar status color")
        print("  cursor_pos <image> <x> <y> <w> <h>      - Detect cursor in terminal")
        print("  check_colors <image> <x> <y> <w> <h>    - Check for multiple colors")
        print("  pixel <image> <x> <y>                   - Get pixel color")
        sys.exit(1)
    
    cmd = sys.argv[1]
    image_path = sys.argv[2]

    if cmd == 'verify_window':
        min_w = int(sys.argv[3]) if len(sys.argv) > 3 else 400
        min_h = int(sys.argv[4]) if len(sys.argv) > 4 else 300
        result = verify_window(image_path, min_width=min_w, min_height=min_h)
        print(f"OK:{result['ok']}")
        print(f"REASON:{result['reason']}")
        print(f"AVG_BRIGHTNESS:{result['avg_brightness']:.1f}")
        print(f"VARIANCE:{result['variance']:.1f}")
        print(f"WIDTH:{result['width']}")
        print(f"HEIGHT:{result['height']}")

    elif cmd == 'sidebar_status':
        x, y, w, h = map(int, sys.argv[3:7])
        result = check_sidebar_status(image_path, (x, y, w, h))
        print(f"STATUS:{result['status']}")
        print(f"CONFIDENCE:{result['confidence']:.2f}")
    
    elif cmd == 'cursor_pos':
        x, y, w, h = map(int, sys.argv[3:7])
        result = detect_cursor(image_path, (x, y, w, h))
        if result:
            print(f"CURSOR_X:{result['x']}")
            print(f"CURSOR_Y:{result['y']}")
            print(f"BRIGHTNESS:{result['brightness']:.2f}")
        else:
            print("CURSOR:NOT_FOUND")
    
    elif cmd == 'check_colors':
        x, y, w, h = map(int, sys.argv[3:7])
        result = has_multiple_colors(image_path, (x, y, w, h))
        print(f"HAS_MULTIPLE_COLORS:{result['has_multiple_colors']}")
        print(f"COLOR_COUNT:{result['color_count']}")
    
    elif cmd == 'pixel':
        x, y = map(int, sys.argv[3:5])
        color = get_pixel_color(image_path, x, y)
        if color:
            print(f"RGB:{color[0]},{color[1]},{color[2]}")
        else:
            print("RGB:ERROR")
    
    else:
        print(f"Unknown command: {cmd}")
        sys.exit(1)
