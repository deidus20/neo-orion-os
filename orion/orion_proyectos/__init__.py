"""Gestión de proyectos para ingeniería y empresas.

Un proyecto es un archivo YAML/JSON en la bóveda (proyectos/<id>.json + nota .md).
Incluye lo que se usa en obra y en Gestión de Proyectos:
  · EDT/WBS jerárquica (1, 1.1, 1.1.1…)
  · Programación CPM: inicio/fin tempranos y tardíos, holgura, RUTA CRÍTICA
  · Carta Gantt en días calendario (SVG en la estética del sistema + Excel)
  · Matriz RACI (Responsable, Aprobador, Consultado, Informado)
  · Presupuesto con estructura chilena: costo directo → gastos generales → utilidad
    → contingencia → neto → IVA 19 % → total
  · Avance real vs. planificado (curva S simple) y estado por tarea
"""
from __future__ import annotations
import datetime as dt, json, os
from dataclasses import dataclass, field, asdict
from pathlib import Path

IVA_CL = 0.19


@dataclass
class Tarea:
    id: str                    # "1.2.3" (EDT)
    nombre: str
    dias: float = 0            # 0 = hito o paquete resumen
    depende: list[str] = field(default_factory=list)
    responsable: str = ""
    raci: dict = field(default_factory=dict)      # {"Ana": "R", "Cliente": "A", ...}
    costo: float = 0           # costo directo CLP (materiales + mano de obra)
    avance: float = 0          # 0..1
    # calculados
    ei: float = 0; ef: float = 0; it: float = 0; ft: float = 0; holgura: float = 0; critica: bool = False


@dataclass
class Proyecto:
    id: str
    nombre: str
    cliente: str = ""
    inicio: str = ""                       # AAAA-MM-DD
    tareas: list[Tarea] = field(default_factory=list)
    gg: float = 0.15; utilidad: float = 0.10; contingencia: float = 0.05
    moneda: str = "CLP"

    # ---------------------------------------------------- CPM
    def programar(self) -> dict:
        hojas = [t for t in self.tareas if not any(o.id.startswith(t.id + ".") for o in self.tareas)]
        por_id = {t.id: t for t in hojas}
        orden, visit = [], set()

        def dfs(t, pila=()):
            if t.id in pila:
                raise ValueError(f"dependencia circular en {t.id}")
            if t.id in visit:
                return
            for d in t.depende:
                if d not in por_id:
                    raise KeyError(f"{t.id} depende de {d}, que no existe (o es un paquete resumen)")
                dfs(por_id[d], pila + (t.id,))
            visit.add(t.id); orden.append(t)
        for t in hojas:
            dfs(t)
        for t in orden:
            t.ei = max([por_id[d].ef for d in t.depende], default=0); t.ef = t.ei + t.dias
        fin = max((t.ef for t in orden), default=0)
        for t in reversed(orden):
            suc = [s for s in orden if t.id in s.depende]
            t.ft = min([s.it for s in suc], default=fin); t.it = t.ft - t.dias
            t.holgura = round(t.it - t.ei, 2); t.critica = abs(t.holgura) < 1e-9
        for p in self.tareas:              # paquetes resumen = envolvente de sus hijas
            hijas = [h for h in hojas if h.id.startswith(p.id + ".")]
            if hijas:
                p.ei, p.ef = min(h.ei for h in hijas), max(h.ef for h in hijas)
                p.dias, p.costo = p.ef - p.ei, sum(h.costo for h in hijas)
                tot = sum(h.dias for h in hijas) or 1
                p.avance = sum(h.avance * h.dias for h in hijas) / tot
                p.critica = any(h.critica for h in hijas)
        ruta = [t.id for t in orden if t.critica]
        return {"duracion_dias": fin, "ruta_critica": ruta,
                "termino": self._fecha(fin) if self.inicio else None}

    def _fecha(self, d: float) -> str:
        return str(dt.date.fromisoformat(self.inicio) + dt.timedelta(days=round(d)))

    # ---------------------------------------------------- presupuesto (Chile)
    def presupuesto(self) -> dict:
        cd = sum(t.costo for t in self.tareas if not any(o.id.startswith(t.id + ".") for o in self.tareas))
        gg, ut = cd * self.gg, cd * self.utilidad
        cont = (cd + gg + ut) * self.contingencia
        neto = cd + gg + ut + cont
        iva = neto * IVA_CL
        r = lambda x: round(x)
        return {"costo_directo": r(cd), "gastos_generales": r(gg), "utilidad": r(ut), "contingencia": r(cont),
                "neto": r(neto), "iva_19": r(iva), "total": r(neto + iva),
                "supuestos": f"GG {self.gg:.0%} y utilidad {self.utilidad:.0%} sobre CD; contingencia {self.contingencia:.0%} sobre CD+GG+U"}

    def raci(self) -> dict:
        personas = sorted({p for t in self.tareas for p in t.raci} | {t.responsable for t in self.tareas if t.responsable})
        filas = {t.id + " " + t.nombre: {p: t.raci.get(p, "R" if p == t.responsable else "") for p in personas} for t in self.tareas}
        problemas = [k for k, v in filas.items() if list(v.values()).count("A") > 1]
        return {"personas": personas, "matriz": filas, "alertas": [f"más de un Aprobador en {k}" for k in problemas]}

    def avance(self, hoy: str | None = None) -> dict:
        self.programar()
        hojas = [t for t in self.tareas if not any(o.id.startswith(t.id + ".") for o in self.tareas)]
        tot = sum(t.dias for t in hojas) or 1
        real = sum(t.avance * t.dias for t in hojas) / tot
        if self.inicio:
            d = (dt.date.fromisoformat(hoy) if hoy else dt.date.today()) - dt.date.fromisoformat(self.inicio)
            plan = sum(min(max(d.days - t.ei, 0), t.dias) for t in hojas) / tot
        else:
            plan = None
        atrasadas = [t.id for t in hojas if plan is not None and t.ef <= (d.days if self.inicio else 0) and t.avance < 1]
        return {"real": round(real, 3), "planificado": round(plan, 3) if plan is not None else None,
                "estado": "al día" if plan is None or real >= plan - .02 else "atrasado", "tareas_atrasadas": atrasadas}

    # ---------------------------------------------------- salidas
    def gantt_svg(self, ancho: int = 980) -> str:
        info = self.programar()
        D = max(info["duracion_dias"], 1)
        fila, izq = 26, 300
        esc = (ancho - izq - 20) / D
        alto = 60 + fila * len(self.tareas)
        s = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {ancho} {alto}" font-family="JetBrains Mono,monospace" font-size="11">',
             f'<rect width="{ancho}" height="{alto}" fill="#101113"/>',
             f'<text x="12" y="22" fill="#cdf564" font-size="13">{self.nombre.upper()}</text>',
             f'<text x="12" y="40" fill="#8b8e94">{info["duracion_dias"]:g} días · ruta crítica en rojo · {self.cliente}</text>']
        paso = max(1, round(D / 12))
        for d in range(0, int(D) + 1, paso):
            x = izq + d * esc
            s.append(f'<line x1="{x:.1f}" y1="48" x2="{x:.1f}" y2="{alto}" stroke="#2a2c30"/><text x="{x + 2:.1f}" y="58" fill="#55585e" font-size="9">d{d}</text>')
        for i, t in enumerate(self.tareas):
            y = 64 + i * fila
            nivel = t.id.count(".")
            resumen = any(o.id.startswith(t.id + ".") for o in self.tareas)
            col = "#ff4a2a" if t.critica and not resumen else ("#e6e8ea" if resumen else "#cdf564")
            s.append(f'<text x="{12 + nivel * 12}" y="{y + 13}" fill="{"#e6e8ea" if resumen else "#b9bcc1"}">{t.id} {t.nombre[:30]}</text>')
            x, w = izq + t.ei * esc, max(t.dias * esc, 3)
            if t.dias == 0:
                s.append(f'<rect x="{x - 5:.1f}" y="{y + 3}" width="10" height="10" fill="#e6e8ea" transform="rotate(45 {x:.1f} {y + 8})"/>')
            elif resumen:
                s.append(f'<rect x="{x:.1f}" y="{y + 6}" width="{w:.1f}" height="5" fill="{col}"/>')
            else:
                s.append(f'<rect x="{x:.1f}" y="{y + 2}" width="{w:.1f}" height="14" fill="none" stroke="{col}"/>'
                         f'<rect x="{x:.1f}" y="{y + 2}" width="{w * t.avance:.1f}" height="14" fill="{col}" opacity=".85"/>')
                if t.holgura > 0:
                    s.append(f'<line x1="{x + w:.1f}" y1="{y + 9}" x2="{x + w + t.holgura * esc:.1f}" y2="{y + 9}" stroke="#55585e" stroke-dasharray="2 3"/>')
        s.append("</svg>")
        return "".join(s)

    def a_excel(self, ruta: str) -> str:
        from openpyxl import Workbook
        from openpyxl.styles import PatternFill, Font, Alignment
        info = self.programar(); wb = Workbook()
        ws = wb.active; ws.title = "Programa"
        cab = ["EDT", "Tarea", "Días", "Depende de", "Responsable", "Inicio temprano", "Fin temprano", "Holgura", "Crítica", "Avance", "Costo directo"]
        ws.append(cab)
        for t in self.tareas:
            ws.append([t.id, t.nombre, t.dias, ", ".join(t.depende), t.responsable,
                       self._fecha(t.ei) if self.inicio else t.ei, self._fecha(t.ef) if self.inicio else t.ef,
                       t.holgura, "sí" if t.critica else "", t.avance, t.costo])
        D = int(info["duracion_dias"]) + 1
        g = wb.create_sheet("Carta Gantt")
        g.append(["EDT", "Tarea"] + [f"d{d}" for d in range(D)])
        lima, rojo = PatternFill("solid", fgColor="CDF564"), PatternFill("solid", fgColor="FF4A2A")
        for r, t in enumerate(self.tareas, start=2):
            g.cell(r, 1, t.id); g.cell(r, 2, t.nombre)
            for d in range(int(t.ei), int(t.ef)):
                g.cell(r, 3 + d).fill = rojo if t.critica else lima
        g.column_dimensions["B"].width = 34
        p = wb.create_sheet("Presupuesto")
        for k, v in self.presupuesto().items():
            p.append([k.replace("_", " ").capitalize(), v])
        rc = wb.create_sheet("RACI"); R = self.raci()
        rc.append(["Tarea"] + R["personas"])
        for k, v in R["matriz"].items():
            rc.append([k] + [v[pp] for pp in R["personas"]])
        for wsx in wb.worksheets:
            for c in wsx[1]:
                c.font = Font(bold=True)
        wb.save(ruta)
        return ruta

    # ---------------------------------------------------- persistencia
    def guardar(self, boveda: str | None = None) -> Path:
        raiz = Path(boveda or os.environ.get("ORION_BOVEDA", os.path.expanduser("~/Orion"))) / "proyectos"
        raiz.mkdir(parents=True, exist_ok=True)
        f = raiz / f"{self.id}.json"
        f.write_text(json.dumps(asdict(self), ensure_ascii=False, indent=2))
        info, pres = self.programar(), self.presupuesto()
        md = raiz / f"{self.id}.md"
        md.write_text(f"---\ntipo: proyecto\ncliente: {self.cliente}\ninicio: {self.inicio}\netiquetas: [proyecto]\n---\n"
                      f"# {self.nombre}\n\n- Duración: {info['duracion_dias']:g} días (término {info['termino']})\n"
                      f"- Ruta crítica: {' → '.join(info['ruta_critica'])}\n- Total con IVA: ${pres['total']:,.0f} {self.moneda}\n\n"
                      "## Tareas\n" + "\n".join(f"- [{'x' if t.avance >= 1 else ' '}] {t.id} {t.nombre} ({t.dias:g} d)" for t in self.tareas) + "\n")
        return f

    @classmethod
    def cargar(cls, ruta: str) -> "Proyecto":
        d = json.loads(Path(ruta).read_text())
        d["tareas"] = [Tarea(**t) for t in d["tareas"]]
        return cls(**d)
