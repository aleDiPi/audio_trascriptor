"""Genera assets/icon.ico e assets/icon-preview.png (richiede Pillow). Uso: python assets/make_icon.py"""
from PIL import Image, ImageDraw
import os

S = 1024  # disegno ad alta risoluzione, poi ridotto


def draw():
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle((32, 32, S - 32, S - 32), radius=230, fill=(37, 99, 235, 255))
    w = (255, 255, 255, 255)
    # microfono: capsula, staffa a U, asta e base
    d.rounded_rectangle((392, 170, 632, 560), radius=120, fill=w)
    d.arc((300, 330, 724, 700), 0, 180, fill=w, width=44)
    d.rectangle((490, 690, 534, 800), fill=w)
    d.rounded_rectangle((390, 790, 634, 834), radius=22, fill=w)
    # onde sonore ai lati
    for r, a in ((110, 255), (190, 170)):
        c = (255, 255, 255, a)
        d.arc((512 - 130 - r, 365 - r, 512 - 130 + r, 365 + r), 140, 220, fill=c, width=36)
        d.arc((512 + 130 - r, 365 - r, 512 + 130 + r, 365 + r), -40, 40, fill=c, width=36)
    return im


here = os.path.dirname(os.path.abspath(__file__))
big = draw()
sizes = [16, 24, 32, 48, 64, 128, 256]
imgs = [big.resize((n, n), Image.LANCZOS) for n in sizes]
imgs[-1].save(os.path.join(here, "icon.ico"), format="ICO", sizes=[(n, n) for n in sizes], append_images=imgs[:-1])
prev = Image.new("RGBA", (256 + 20 + 128 + 20 + 48 + 20 + 16, 276), (245, 245, 245, 255))
x = 10
for n in (256, 128, 48, 16):
    im = big.resize((n, n), Image.LANCZOS)
    prev.paste(im, (x, 10 + (256 - n) // 2), im)
    x += n + 20
prev.save(os.path.join(here, "icon-preview.png"))
