"""Dictado de documentos: hablar y que salga un Word con formato (informe DUOC, memoria explicativa, carta, acta).

Cómo se dicta (frases de control, en cualquier parte del dictado):
  «título …»               → título del documento (los títulos terminan en la siguiente marca o en «punto aparte»)
  «sección …» / «capítulo …» → título de sección (se numera solo)
  «subtítulo …»            → subsección
  «punto …» / «viñeta …»   → elemento de lista
  «punto aparte»           → párrafo nuevo
  «tabla: a, b, c»         → fila de tabla (la primera es la cabecera)
Si hay una IA local, primero ordena el dictado (quita muletillas, arregla puntuación) sin inventar contenido.

Plantillas: informe-duoc (portada con el logo de DUOC UC arriba a la derecha si existe
~/.orion/plantillas/duoc-logo.png; asignatura, docente, integrantes, fecha), memoria-electrica (estructura de
memoria explicativa para la SEC: generalidades, descripción, cálculos, especificaciones, planos), carta y acta.
"""
from __future__ import annotations

import datetime as dt
import re
from pathlib import Path

PLANTILLAS = {
    "informe-duoc": ["Introducción", "Desarrollo", "Conclusiones", "Bibliografía"],
    "memoria-electrica": ["Generalidades", "Descripción de la instalación", "Cálculos justificativos",
                          "Especificaciones técnicas", "Cubicación de materiales", "Planos"],
    "carta": [],
    "acta": ["Asistentes", "Temas tratados", "Acuerdos", "Próximos pasos"],
}
LOGO_DUOC = Path("~/.orion/plantillas/duoc-logo.png").expanduser()
MULETILLAS = re.compile(r"\b(eh+|em+|mmm+|o sea|cachai|ya po|entonces eh)\b[,]?\s*", re.I)


def ordenar(dictado: str) -> list[tuple[str, str]]:
    """Dictado → bloques (tipo, texto). tipo: titulo | seccion | subseccion | parrafo | punto | tabla."""
    t = MULETILLAS.sub("", dictado)
    t = re.sub(r"\s+", " ", t).strip()
    marcas = r"(t[ií]tulo|secci[oó]n|cap[ií]tulo|subt[ií]tulo|punto aparte|punto|viñeta|tabla)\b[:,]?"
    piezas = re.split(r"(?i)\b" + marcas, t)
    bloques: list[tuple[str, str]] = []
    actual = "parrafo"
    for p in piezas:
        if p is None:
            continue
        k = p.lower().strip()
        if re.fullmatch(marcas.replace("\\b[:,]?", ""), k, re.I):
            actual = {"titulo": "titulo", "título": "titulo", "seccion": "seccion", "sección": "seccion", "capitulo": "seccion",
                      "capítulo": "seccion", "subtitulo": "subseccion", "subtítulo": "subseccion", "punto aparte": "parrafo",
                      "punto": "punto", "viñeta": "punto", "tabla": "tabla"}[k]
            continue
        txt = p.strip(" .,:;")
        if not txt:
            continue
        txt = txt[0].upper() + txt[1:]
        if actual in ("parrafo", "punto") and not txt.endswith((".", "?", "!")):
            txt += "."
        bloques.append((actual, txt))
        if actual in ("titulo", "seccion", "subseccion", "tabla"):
            actual = "parrafo"
    return bloques


def documento(bloques: list[tuple[str, str]], ruta: str, plantilla: str = "informe-duoc", datos: dict | None = None) -> str:
    """Arma el .docx (python-docx). datos: asignatura, docente, integrantes, seccion, fecha, destinatario…"""
    from docx import Document
    from docx.enum.text import WD_ALIGN_PARAGRAPH
    from docx.shared import Cm, Pt, RGBColor
    datos = datos or {}
    d = Document()
    for s in d.sections:
        s.left_margin = s.right_margin = Cm(2.5)
        s.top_margin = s.bottom_margin = Cm(2.5)
    estilo = d.styles["Normal"]
    estilo.font.name, estilo.font.size = "Arial", Pt(11)
    titulo = next((t for k, t in bloques if k == "titulo"), datos.get("titulo", "Documento"))
    if plantilla == "informe-duoc":
        cab = d.add_paragraph()
        cab.alignment = WD_ALIGN_PARAGRAPH.RIGHT
        if LOGO_DUOC.exists():
            cab.add_run().add_picture(str(LOGO_DUOC), width=Cm(4.5))
        else:
            r = cab.add_run("[logo DUOC UC]")
            r.font.color.rgb = RGBColor(0x99, 0x99, 0x99)
        for _ in range(6):
            d.add_paragraph()
        p = d.add_paragraph()
        p.alignment = WD_ALIGN_PARAGRAPH.CENTER
        r = p.add_run(titulo.upper())
        r.bold, r.font.size = True, Pt(20)
        for etiqueta, k in [("Asignatura", "asignatura"), ("Sección", "seccion"), ("Docente", "docente"),
                            ("Integrantes", "integrantes"), ("Fecha", "fecha")]:
            v = datos.get(k) or (dt.date.today().strftime("%d-%m-%Y") if k == "fecha" else "")
            if v:
                q = d.add_paragraph()
                q.alignment = WD_ALIGN_PARAGRAPH.CENTER
                q.add_run(f"{etiqueta}: ").bold = True
                q.add_run(", ".join(v) if isinstance(v, list) else str(v))
        d.add_page_break()
    elif plantilla == "carta":
        d.add_paragraph(datos.get("lugar", "Santiago") + ", " + dt.date.today().strftime("%d-%m-%Y")).alignment = WD_ALIGN_PARAGRAPH.RIGHT
        if datos.get("destinatario"):
            d.add_paragraph(datos["destinatario"])
    else:
        d.add_heading(titulo, 0)
    n_sec = n_sub = 0
    tabla = None
    for k, t in bloques:
        if k == "titulo":
            continue
        if k != "tabla":
            tabla = None
        if k == "seccion":
            n_sec += 1
            n_sub = 0
            d.add_heading(f"{n_sec}. {t}", 1)
        elif k == "subseccion":
            n_sub += 1
            d.add_heading(f"{n_sec}.{n_sub} {t}", 2)
        elif k == "punto":
            d.add_paragraph(t, style="List Bullet")
        elif k == "tabla":
            celdas = [c.strip() for c in re.split(r",|;", t) if c.strip()]
            if tabla is None:
                tabla = d.add_table(rows=1, cols=len(celdas))
                tabla.style = "Table Grid"
                for c, v in zip(tabla.rows[0].cells, celdas):
                    c.text = v
                    c.paragraphs[0].runs[0].bold = True
            else:
                fila = tabla.add_row().cells
                for c, v in zip(fila, celdas):
                    c.text = v
        else:
            d.add_paragraph(t)
    if plantilla == "carta" and datos.get("firma"):
        d.add_paragraph()
        d.add_paragraph(datos["firma"])
    Path(ruta).parent.mkdir(parents=True, exist_ok=True)
    d.save(ruta)
    return ruta


def esqueleto(plantilla: str) -> list[tuple[str, str]]:
    """Secciones vacías de la plantilla, para dictar encima."""
    return [("seccion", s) for s in PLANTILLAS.get(plantilla, [])]


def pulir_con_ia(dictado: str, charlar=None) -> str:
    """Pasa el dictado por la IA local para ordenar la redacción SIN agregar contenido. Si no hay IA, igual."""
    if charlar is None:
        try:
            from orion_core.servidor import charlar
        except Exception:
            return dictado
    r = charlar("Corrige solo puntuación, mayúsculas y muletillas de este dictado. No agregues ni quites ideas, "
                "conserva las palabras «título», «sección», «subtítulo», «punto» y «tabla». Devuelve solo el texto:\n\n" + dictado)
    if r.get("sin_ia") or r.get("error"):
        return dictado
    return r.get("texto") or dictado
