"""Regenerate the sample's small bitmap sign atlas with Pillow's bundled font."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

font = ImageFont.load_default(size=16)
rows = []
for code in range(32, 96):
    image = Image.new("L", (16, 20))
    draw = ImageDraw.Draw(image)
    box = draw.textbbox((0, 0), chr(code), font=font)
    draw.text(((16 - box[2] + box[0]) // 2 - box[0], 1 - box[1]), chr(code), font=font, fill=255)
    rows.extend(sum((image.getpixel((x, y)) >= 96) << x for x in range(16)) for y in range(20))
words = ["NIGHT CITY", "KIROSHI", "NEURAL", "RAMEN 24H", "ION HOTEL", "AFTERLIFE", "ZENITH", "OFFWORLD"]
for word in words:
    rows.extend(ord(c) - 32 for c in word.ljust(12))
lines = ["    " + ", ".join(f"{v}u" for v in rows[i:i + 16]) + "," for i in range(0, len(rows), 16)]
output = "// Rasterized sign glyphs; regenerate with make_sign_font.py (Pillow).\npublic fun sign_font(): uint[] {\n    return [\n" + "\n".join(lines) + "\n    ];\n}\n"
Path(__file__).with_name("room_signs.dream").write_text(output, encoding="utf-8")
