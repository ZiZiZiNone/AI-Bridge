"""生成 src-tauri/icons/ 的占位图标（骨架期用）。

用途：Tauri 打包（bundle.icon）与托盘需要真实存在的图标文件，
骨架期先给一套品牌色占位图，替换为设计稿后重新运行本脚本即可。

依赖：Pillow（`pip install pillow`）——仅本脚本需要，前端与 Rust 无此依赖。
用法：python scripts/make-icons.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ICON_DIR = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"
BRAND = (47, 111, 237, 255)  # 与 src/styles/app.css 的 --ab-accent 一致
INK = (255, 255, 255, 255)
ICO_SIZES = [16, 32, 48, 64, 128, 256]


def render(size: int) -> Image.Image:
    """画一枚纯几何占位标：品牌色圆角方块 + 白色连线（桥）。"""
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)

    radius = max(2, size // 5)
    draw.rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=BRAND)

    stroke = max(1, size // 12)
    left = (int(size * 0.22), int(size * 0.34))
    right = (int(size * 0.78), int(size * 0.66))
    mid_top = (int(size * 0.5), int(size * 0.5))
    draw.line((left, mid_top, right), fill=INK, width=stroke, joint="curve")
    node = max(2, size // 10)
    for point in (left, right):
        draw.ellipse(
            (point[0] - node, point[1] - node, point[0] + node, point[1] + node),
            fill=INK,
        )

    if size >= 64:
        font = ImageFont.load_default(size=max(10, size // 6))
        text = "AI-Bridge"
        box = draw.textbbox((0, 0), text, font=font)
        draw.text(
            ((size - (box[2] - box[0])) / 2, size * 0.74),
            text,
            font=font,
            fill=INK,
        )

    return image


def main() -> None:
    ICON_DIR.mkdir(parents=True, exist_ok=True)

    for name, size in (("32x32.png", 32), ("128x128.png", 128), ("icon.png", 512)):
        path = ICON_DIR / name
        render(size).save(path, format="PNG")
        print(f"wrote {path} ({size}x{size})")

    ico_path = ICON_DIR / "icon.ico"
    render(256).save(ico_path, format="ICO", sizes=[(s, s) for s in ICO_SIZES])
    print(f"wrote {ico_path} (sizes={ICO_SIZES})")


if __name__ == "__main__":
    main()
