"""Regenerate checked-in PNG/ICO from the SVG primitives. Requires Pillow (development only)."""
from pathlib import Path
import xml.etree.ElementTree as ET
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parent.parent / "assets"
svg = ET.parse(root / "loafing-reader.svg").getroot()
scale = 4
image = Image.new("RGBA", (256 * scale, 256 * scale))
draw = ImageDraw.Draw(image)
for node in svg:
    a = node.attrib
    tag = node.tag.rsplit("}", 1)[-1]
    if tag == "rect":
        x, y, w, h = [float(a[k]) * scale for k in ("x", "y", "width", "height")]
        draw.rounded_rectangle((x, y, x + w, y + h), radius=float(a["rx"]) * scale, fill=a["fill"])
    elif tag == "polygon":
        points = [tuple(float(v) * scale for v in pair.split(",")) for pair in a["points"].split()]
        draw.polygon(points, fill=a["fill"])
    elif tag == "line":
        xy = [float(a[k]) * scale for k in ("x1", "y1", "x2", "y2")]
        draw.line(xy, fill=a["stroke"], width=int(a["stroke-width"]) * scale)
    else:
        raise ValueError(f"Unsupported SVG primitive: {tag}")
image = image.resize((256, 256), Image.Resampling.LANCZOS)
image.save(root / "loafing-reader.png")
image.save(root / "loafing-reader.ico", sizes=[(n,n) for n in (16,20,24,32,40,48,64,128,256)])
