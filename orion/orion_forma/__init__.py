"""Cadena foto/idea → imagen → modelo 3D → CAD → servos integrados.

    "dame alternativas"  → imagen   (FLUX / SD / nube)              [GPU]
    "pásala a 3D"        → malla    (TRELLIS.2 / Hunyuan3D / TripoSR) [GPU]  o  silueta extruida [CPU]
    "llévala a CAD"      → sólido   (build123d, STEP + STL a escala real)
    "integra los servos" → bolsillos, ejes y tornillos para cada servo
Cada paso usa la alternativa que el equipo aguanta (orion_core/perfil.py); el
paso CPU (silueta) funciona en cualquier equipo y sirve de respaldo.
"""
from __future__ import annotations
import json, logging, os, shutil, subprocess
logging.getLogger("build123d").setLevel(logging.WARNING)
from pathlib import Path

# Medidas de cuerpo (largo × ancho × alto, mm), entre-ejes de orejas y Ø de tornillo.
# Valores típicos de catálogo: confirmar con la hoja de datos del servo que compres.
SERVOS = {
    "MG996R":  {"cuerpo": (40.7, 19.7, 37.0), "orejas": 49.5, "tornillo": 4.2, "eje_desde_borde": 10.0},
    "SG90":    {"cuerpo": (22.5, 12.0, 22.7), "orejas": 27.5, "tornillo": 2.2, "eje_desde_borde": 6.0},
    "STS3215": {"cuerpo": (45.2, 24.7, 35.0), "orejas": None, "tornillo": 2.5, "eje_desde_borde": 12.0},
    "NEMA17":  {"cuerpo": (42.3, 42.3, 40.0), "orejas": 31.0, "tornillo": 3.4, "eje_desde_borde": 21.15},
}


# ------------------------------------------------------------------ 1. imágenes alternativas
def alternativas(descripcion: str, n: int = 3, motor: str = "auto", salida: str = "~/orion/forma") -> dict:
    """Genera n imágenes de variantes. Usa diffusers si hay GPU; si no, lo informa (no inventa)."""
    out = Path(os.path.expanduser(salida)); out.mkdir(parents=True, exist_ok=True)
    try:
        import torch
        from diffusers import AutoPipelineForText2Image
        if not torch.cuda.is_available():
            raise RuntimeError("sin GPU")
        modelo = "black-forest-labs/FLUX.1-schnell" if motor in ("auto", "flux") else motor
        pipe = AutoPipelineForText2Image.from_pretrained(modelo, torch_dtype=torch.bfloat16).to("cuda")
        rutas = []
        for i in range(n):
            img = pipe(f"{descripcion}, variante {i + 1}, render de producto, fondo neutro", num_inference_steps=4, guidance_scale=0).images[0]
            p = out / f"alternativa_{i + 1}.png"; img.save(p); rutas.append(str(p))
        return {"ok": True, "imagenes": rutas, "motor": modelo}
    except Exception as e:
        return {"ok": False, "motivo": f"generación de imágenes no disponible en este equipo ({e}); usa la central con GPU o la nube"}


# ------------------------------------------------------------------ 2. imagen → malla
def foto_a_malla(imagen: str, motor: str = "auto", salida: str = "~/orion/forma") -> dict:
    """Modelos abiertos instalados como herramientas externas (se detectan solos)."""
    out = Path(os.path.expanduser(salida)); out.mkdir(parents=True, exist_ok=True)
    candidatos = {"trellis.2": "trellis2-cli", "hunyuan3d": "hunyuan3d-cli", "triposr": "triposr"}
    for nombre, binario in candidatos.items():
        if motor not in ("auto", nombre) or not shutil.which(binario):
            continue
        dest = out / (Path(imagen).stem + f"_{nombre}.glb")
        r = subprocess.run([binario, imagen, "--output", str(dest)], capture_output=True, text=True)
        if r.returncode == 0:
            return {"ok": True, "malla": str(dest), "motor": nombre}
    return {"ok": False, "motivo": "sin modelo foto→3D instalado; uso silueta_a_solido (CPU) como respaldo"}


def silueta_a_solido(imagen: str, alto_real_mm: float, espesor_mm: float = 10.0, nombre: str = "",
                     salida: str = "~/orion/forma", tolerancia: float = 0.004) -> dict:
    """Respaldo CPU: contorno principal de la foto → polígono a escala → sólido extruido."""
    import cv2
    from build123d import BuildPart, BuildSketch, Polyline, make_face, extrude, export_step, export_stl, BuildLine
    img = cv2.imread(imagen, cv2.IMREAD_GRAYSCALE)
    if img is None:
        return {"ok": False, "motivo": f"no pude leer {imagen}"}
    blur = cv2.GaussianBlur(img, (5, 5), 0)
    _, th = cv2.threshold(blur, 0, 255, cv2.THRESH_BINARY + cv2.THRESH_OTSU)
    if th[0, 0] > 127:                       # fondo claro → invertir
        th = 255 - th
    cnts, _ = cv2.findContours(th, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_NONE)
    c = max(cnts, key=cv2.contourArea)
    c = cv2.approxPolyDP(c, tolerancia * cv2.arcLength(c, True), True)[:, 0, :]
    x, y, w, h = cv2.boundingRect(c)
    k = alto_real_mm / h                       # mm por píxel
    pts = [((px - x) * k, (y + h - py) * k) for px, py in c]   # eje Y hacia arriba
    with BuildPart() as bp:
        with BuildSketch():
            with BuildLine():
                Polyline(*pts, close=True)
            make_face()
        extrude(amount=espesor_mm)
    pieza = bp.part
    nombre = nombre or Path(imagen).stem
    return _exportar(pieza, nombre, salida) | {"vertices_contorno": len(pts), "mm_por_pixel": round(k, 4)}


# ------------------------------------------------------------------ 3. integrar servos
def integrar_servos(paso_step: str, servos: list[dict], nombre: str = "", salida: str = "~/orion/forma") -> dict:
    """servos: [{"modelo": "MG996R", "x": mm, "y": mm, "rot": grados, "eje": "A2"}].
    Corta el bolsillo del cuerpo, el paso del eje y los agujeros de las orejas."""
    from build123d import (import_step, Box, Cylinder, Pos, Rot, Align)
    pieza = import_step(os.path.expanduser(paso_step))
    bb = pieza.bounding_box()
    zmax = bb.max.Z
    informe = []
    for s in servos:
        d = SERVOS[s["modelo"]]
        L, W, H = d["cuerpo"]
        prof = min(H, bb.size.Z - 2)            # deja 2 mm de piso
        loc = Pos(s["x"], s["y"], zmax) * Rot(0, 0, s.get("rot", 0))
        pieza -= loc * Box(L + .4, W + .4, prof, align=(Align.CENTER, Align.CENTER, Align.MAX))   # 0,2 mm de holgura
        eje_x = -L / 2 + d["eje_desde_borde"]
        pieza -= loc * Pos(eje_x, 0, -prof) * Cylinder(4, bb.size.Z, align=(Align.CENTER, Align.CENTER, Align.MAX))
        if d["orejas"]:
            for sx in (-1, 1):
                for sy in (-1, 1):
                    pieza -= loc * Pos(sx * d["orejas"] / 2, sy * W / 4, 0) * Cylinder(d["tornillo"] / 2, 12, align=(Align.CENTER, Align.CENTER, Align.MAX))
        informe.append({"eje": s.get("eje"), "modelo": s["modelo"], "bolsillo_mm": [round(L + .4, 1), round(W + .4, 1), round(prof, 1)]})
    nombre = nombre or Path(paso_step).stem + "_servos"
    return _exportar(pieza, nombre, salida) | {"servos": informe}


def _exportar(pieza, nombre: str, salida: str) -> dict:
    from build123d import export_step, export_stl
    out = Path(os.path.expanduser(salida)); out.mkdir(parents=True, exist_ok=True)
    step, stl = out / f"{nombre}.step", out / f"{nombre}.stl"
    export_step(pieza, str(step)); export_stl(pieza, str(stl))
    bb = pieza.bounding_box()
    return {"ok": True, "step": str(step), "stl": str(stl),
            "medidas_mm": [round(bb.size.X, 1), round(bb.size.Y, 1), round(bb.size.Z, 1)],
            "volumen_cm3": round(pieza.volume / 1000, 2), "masa_pla_g": round(pieza.volume / 1000 * 1.24, 1)}


def vista_previa(stl: str, png: str, color: str = "#cdf564") -> str:
    """PNG en la estética del sistema (para el HUD o para mandarlo por chat)."""
    import trimesh, matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from mpl_toolkits.mplot3d.art3d import Poly3DCollection
    m = trimesh.load(stl)
    fig = plt.figure(figsize=(6, 6), facecolor="#101113")
    ax = fig.add_subplot(projection="3d", facecolor="#101113")
    tris = m.vertices[m.faces]
    ax.add_collection3d(Poly3DCollection(tris, facecolor="#1d1f22", edgecolor=color, linewidths=0.35, alpha=.95))
    mn, mx = m.bounds
    c, r = (mn + mx) / 2, (mx - mn).max() / 2
    ax.set_xlim(c[0] - r, c[0] + r); ax.set_ylim(c[1] - r, c[1] + r); ax.set_zlim(c[2] - r, c[2] + r)
    ax.view_init(52, -60); ax.set_axis_off(); ax.set_box_aspect((1, 1, 1), zoom=1.45)
    fig.text(.04, .95, Path(stl).stem.upper().replace("_", " "), color=color, family="monospace", fontsize=11)
    fig.text(.04, .91, f"{(mx - mn)[0]:.0f} × {(mx - mn)[1]:.0f} × {(mx - mn)[2]:.0f} mm", color="#8b8e94", family="monospace", fontsize=9)
    fig.savefig(png, dpi=140, bbox_inches="tight", facecolor="#101113"); plt.close(fig)
    return png
