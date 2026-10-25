"""Genera assets/icon.ico e assets/icon-preview.png (richiede Pillow). Uso: python assets/make_icon.py"""
from PIL import Image, ImageDraw, ImageFilter, ImageChops
import os

S = 1024  # disegno ad alta risoluzione, poi ridotto
TOP = (99, 102, 241)      # indaco
MID = (139, 92, 246)      # viola
BOT = (236, 72, 153)      # rosa


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def gradient():
    """Gradiente diagonale indaco -> viola -> rosa."""
    g = Image.new("RGB", (S, S))
    px = g.load()
    for y in range(S):
        for x in range(S):
            t = (x * 0.35 + y * 0.65) / S
            px[x, y] = lerp(TOP, MID, t / 0.5) if t < 0.5 else lerp(MID, BOT, (t - 0.5) / 0.5)
    return g


def draw():
    m = 40
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle((m, m, S - m, S - m), radius=235, fill=255)
    bg = gradient().convert("RGBA")
    # riflesso morbido in alto
    gloss = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(gloss).ellipse((-200, -520, S + 200, 430), fill=(255, 255, 255, 46))
    bg = Image.alpha_composite(bg, gloss)

    fg = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(fg)
    w = (255, 255, 255, 255)
    # microfono: capsula + staffa + asta, tratto pieno e spesso
    d.rounded_rectangle((396, 190, 628, 560), radius=116, fill=w)
    d.arc((318, 330, 706, 718), 0, 180, fill=w, width=46)
    d.rounded_rectangle((489, 700, 535, 790), radius=23, fill=w)
    d.rounded_rectangle((405, 770, 619, 816), radius=23, fill=w)
    # barre della forma d'onda ai lati (voce)
    for dx, h in ((-190, 130), (-290, 220), (190, 130), (290, 220)):
        cx, cy = 512 + dx, 375
        a = 255 if abs(dx) < 250 else 190
        d.rounded_rectangle((cx - 24, cy - h // 2, cx + 24, cy + h // 2), radius=24, fill=(255, 255, 255, a))
    # ombra morbida sotto il disegno bianco
    sh = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    sh.paste((40, 20, 110, 110), mask=fg.split()[3].filter(ImageFilter.GaussianBlur(22)))
    sh = ImageChops.offset(sh, 0, 16)
    bg = Image.alpha_composite(bg, sh)
    bg = Image.alpha_composite(bg, fg)
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(bg, mask=mask)
    return out


here = os.path.dirname(os.path.abspath(__file__))
big = draw()
sizes = [16, 24, 32, 48, 64, 128, 256]
imgs = [big.resize((n, n), Image.LANCZOS) for n in sizes]
imgs[-1].save(os.path.join(here, "icon.ico"), format="ICO", sizes=[(n, n) for n in sizes], append_images=imgs[:-1])
prev = Image.new("RGBA", (256 + 20 + 128 + 20 + 48 + 20 + 16 + 20, 276), (245, 245, 245, 255))
x = 10
for n in (256, 128, 48, 16):
    im = big.resize((n, n), Image.LANCZOS)
    prev.paste(im, (x, 10 + (256 - n) // 2), im)
    x += n + 20
prev.save(os.path.join(here, "icon-preview.png"))
