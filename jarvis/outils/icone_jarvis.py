"""Dessine LA nouvelle icône de Jarvis (une seule pour tout : Jarvis.exe, la barre, la zone de notification,
les raccourcis, l'installeur et l'extension Chrome).

Style « verre dépoli » : carré arrondi bleu nuit -> bleu glacier, reflet de verre en haut, flocon de Givre
en filigrane, et un grand « J » blanc au centre.

    .venv\\Scripts\\python.exe outils\\icone_jarvis.py
"""
import math
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

RACINE = Path(__file__).resolve().parent.parent
TAILLES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
HAUT, BAS = (14, 42, 110), (56, 189, 248)      # bleu nuit -> bleu glacier


def _degrade(n: int) -> Image.Image:
    g = Image.new("RGBA", (n, n))
    px = g.load()
    for y in range(n):
        for x in range(n):
            t = min(1.0, max(0.0, (0.75 * y + 0.25 * x) / n))       # diagonal doux
            px[x, y] = tuple(int(HAUT[i] + (BAS[i] - HAUT[i]) * t) for i in range(3)) + (255,)
    return g


def _flocon(d: ImageDraw.ImageDraw, cx, cy, r, couleur, epaisseur):
    for k in range(6):
        a = math.pi / 2 + k * math.pi / 3
        ex, ey = cx + r * math.cos(a), cy - r * math.sin(a)
        d.line((cx, cy, ex, ey), fill=couleur, width=epaisseur)
        for f, lg in ((0.55, 0.30), (0.8, 0.2)):              # petites branches en V
            bx, by = cx + r * f * math.cos(a), cy - r * f * math.sin(a)
            for s in (-1, 1):
                b = a + s * math.pi / 4
                d.line((bx, by, bx + r * lg * math.cos(b), by - r * lg * math.sin(b)), fill=couleur, width=epaisseur)
        d.ellipse((ex - epaisseur, ey - epaisseur, ex + epaisseur, ey + epaisseur), fill=couleur)


def dessiner(taille: int) -> Image.Image:
    """Dessine en 4x puis réduit (bords lisses). Sous 32 px : pas de flocon (illisible), J plus gros."""
    n = max(taille, 64) * 4
    petit = taille < 32
    marge = int(n * (0.02 if petit else 0.04))
    rayon = int(n * 0.24)
    forme = Image.new("L", (n, n), 0)
    ImageDraw.Draw(forme).rounded_rectangle((marge, marge, n - marge - 1, n - marge - 1), rayon, fill=255)

    img = _degrade(n)
    calque = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(calque)
    if not petit:                                       # flocon de Givre en filigrane, décalé en bas à droite
        _flocon(d, n * 0.70, n * 0.70, n * 0.30, (255, 255, 255, 60), max(2, n // 70))
    img = Image.alpha_composite(img, calque)

    # reflet de verre : un grand ovale blanc très léger sur le haut
    reflet = Image.new("L", (n, n), 0)
    ImageDraw.Draw(reflet).ellipse((-n * 0.3, -n * 0.75, n * 1.3, n * 0.42), fill=70)
    reflet = reflet.filter(ImageFilter.GaussianBlur(n / 40))
    blanc = Image.new("RGBA", (n, n), (255, 255, 255, 0))
    blanc.putalpha(reflet)
    img = Image.alpha_composite(img, blanc)

    # le « J », avec une ombre douce
    try:
        police = ImageFont.truetype("segoeuib.ttf", int(n * (0.80 if petit else 0.66)))
    except OSError:
        police = ImageFont.load_default()
    centre = (n * 0.47, n * 0.50)
    ombre = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    ImageDraw.Draw(ombre).text((centre[0] + n * 0.015, centre[1] + n * 0.03), "J", font=police,
                               fill=(5, 20, 60, 150), anchor="mm")
    img = Image.alpha_composite(img, ombre.filter(ImageFilter.GaussianBlur(n / 60)))
    ImageDraw.Draw(img).text(centre, "J", font=police, fill=(255, 255, 255, 255), anchor="mm")

    # liseré clair (le bord du verre)
    bord = Image.new("L", (n, n), 0)
    e = max(2, n // (40 if petit else 64))
    ImageDraw.Draw(bord).rounded_rectangle((marge, marge, n - marge - 1, n - marge - 1), rayon, outline=150, width=e)
    liseré = Image.new("RGBA", (n, n), (255, 255, 255, 0))
    liseré.putalpha(bord)
    img = Image.alpha_composite(img, liseré)

    img.putalpha(ImageChops.multiply(img.getchannel("A"), forme))
    return img.resize((taille, taille), Image.LANCZOS)


def main():
    images = [dessiner(t) for t in TAILLES]
    for cible in (RACINE / "lanceur" / "jarvis.ico", RACINE / "barre" / "jarvis.ico"):
        images[-1].save(cible, sizes=[(t, t) for t in TAILLES], append_images=images[:-1])
        print("écrit :", cible)
    dessiner(256).save(RACINE / "outils" / "icone-apercu.png")
    ext = RACINE / "extension-chrome"
    if ext.is_dir():
        for t in (16, 48, 128):
            dessiner(t).save(ext / f"icone-{t}.png")
    print("Icônes créées.")


if __name__ == "__main__":
    main()
