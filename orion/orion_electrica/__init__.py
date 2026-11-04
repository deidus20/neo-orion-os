"""Calculadora eléctrica (Chile, pliegos RIC de la SEC) para usar por voz: «¿qué cable para 20 A a 35 metros?».

Lo que calcula, con las reglas verificadas de los pliegos:
  · RIC N°03 5.1.2: caída de tensión ≤ 3 % en alimentadores y subalimentadores (corriente máxima de diseño)
  · RIC N°03 5.1.3: caída total ≤ 5 % en el punto más desfavorable
  · RIC N°03 5.1.2: sección mínima 4 mm² en alimentadores y 2,5 mm² en subalimentadores
  · caída de tensión monofásica ΔV = 2·L·I·ρ/S · trifásica ΔV = √3·L·I·ρ/S (cos φ = 1, conservador en BT)
  · protección: In ≥ Ib (corriente de diseño) e In ≤ Iz (capacidad del conductor)

La capacidad de transporte (Iz) depende del conductor, la canalización y la temperatura: se toma de la tabla del
RIC N°04 o del fabricante. Si no se entrega, el cálculo de la protección queda pendiente y se avisa; Neo Orión
no inventa ese valor.
"""
from __future__ import annotations

import math
from dataclasses import dataclass, field

RHO = {"cobre": 0.018, "aluminio": 0.029}        # Ω·mm²/m (valores usados en Chile para proyectos)
SECCIONES = [1.5, 2.5, 4, 6, 10, 16, 25, 35, 50, 70, 95, 120, 150, 185, 240, 300]
PROTECCIONES = [6, 10, 16, 20, 25, 32, 40, 50, 63, 80, 100, 125, 160, 200, 250]
LIMITE = {"alimentador": 3.0, "subalimentador": 3.0, "total": 5.0}
SECCION_MIN = {"alimentador": 4.0, "subalimentador": 2.5}
FUENTE = "RIC N°03 (SEC) 5.1.2 y 5.1.3"


@dataclass
class Resultado:
    tension_v: float
    fases: int
    corriente_a: float
    largo_m: float
    material: str
    tipo: str
    seccion_mm2: float
    caida_v: float
    caida_pct: float
    limite_pct: float
    cumple: bool
    proteccion_a: int | None = None
    iz_a: float | None = None
    avisos: list[str] = field(default_factory=list)

    def texto(self) -> str:
        s = (f"{self.seccion_mm2:g} mm² de {self.material}: caída {self.caida_v:.2f} V ({self.caida_pct:.2f} %), "
             f"límite {self.limite_pct:g} % ({FUENTE}) → {'cumple' if self.cumple else 'NO cumple'}.")
        if self.proteccion_a:
            s += f" Protección {self.proteccion_a} A."
        return s + ("" if not self.avisos else " " + " ".join(self.avisos))


def caida(corriente_a: float, largo_m: float, seccion_mm2: float, fases: int = 1, material: str = "cobre") -> float:
    """Caída de tensión en volts (largo = distancia de ida, en metros)."""
    if corriente_a < 0 or largo_m < 0 or seccion_mm2 <= 0:
        raise ValueError("corriente y largo no pueden ser negativos; la sección debe ser mayor que 0")
    k = 2 if fases == 1 else math.sqrt(3)
    return k * largo_m * corriente_a * RHO[material] / seccion_mm2


def corriente(potencia_w: float, tension_v: float = 220, fases: int = 1, cos_phi: float = 1.0) -> float:
    """Corriente de diseño desde la potencia."""
    if fases == 1:
        return potencia_w / (tension_v * cos_phi)
    return potencia_w / (math.sqrt(3) * tension_v * cos_phi)


def demanda_vivienda(potencia_instalada_kw: float) -> float:
    """Demanda de alumbrado en viviendas: primeros 3 kW al 100 %, el exceso al 35 % (tabla de factores del RIC N°03)."""
    p = max(0.0, potencia_instalada_kw)
    return min(p, 3.0) + max(0.0, p - 3.0) * 0.35


def seccion(corriente_a: float, largo_m: float, tension_v: float = 220, fases: int = 1, material: str = "cobre",
            tipo: str = "subalimentador", limite_pct: float | None = None, iz_a: float | None = None,
            caida_previa_pct: float = 0.0) -> Resultado:
    """La sección comercial más chica que cumple la caída (y el mínimo del RIC N°03 según el tipo).

    tipo: alimentador | subalimentador | circuito. caida_previa_pct: lo que ya cae aguas arriba, para revisar el 5 % total.
    iz_a: capacidad del conductor elegido (tabla RIC N°04 / fabricante) para verificar la protección.
    """
    if fases not in (1, 3):
        raise ValueError("fases: 1 o 3")
    if material not in RHO:
        raise ValueError("material: cobre o aluminio")
    lim = limite_pct if limite_pct is not None else LIMITE.get(tipo, 3.0)
    minimo = SECCION_MIN.get(tipo, 1.5)
    avisos: list[str] = []
    elegida = None
    for s in SECCIONES:
        if s < minimo:
            continue
        pct = caida(corriente_a, largo_m, s, fases, material) / tension_v * 100
        if pct <= lim and pct + caida_previa_pct <= LIMITE["total"]:
            elegida = s
            break
    if elegida is None:
        elegida = SECCIONES[-1]
        avisos.append("Ni la sección más grande de la tabla cumple: acorta el recorrido, sube la tensión o divide la carga.")
    v = caida(corriente_a, largo_m, elegida, fases, material)
    pct = v / tension_v * 100
    if elegida == minimo and tipo in SECCION_MIN:
        avisos.append(f"Se usa el mínimo del RIC N°03 para {tipo}: {minimo:g} mm².")
    if caida_previa_pct:
        avisos.append(f"Caída acumulada {pct + caida_previa_pct:.2f} % (máx. 5 % total).")
    prot = next((p for p in PROTECCIONES if p >= corriente_a), None)
    if iz_a is None:
        avisos.append("Falta Iz del conductor (tabla del RIC N°04 o del fabricante) para verificar In ≤ Iz.")
    elif prot is not None and prot > iz_a:
        avisos.append(f"La protección de {prot} A supera la capacidad del conductor ({iz_a:g} A): sube la sección.")
    cumple = pct <= lim and pct + caida_previa_pct <= LIMITE["total"]
    return Resultado(tension_v, fases, round(corriente_a, 2), largo_m, material, tipo, elegida, round(v, 2), round(pct, 2),
                     lim, cumple, prot, iz_a, avisos)


def verificar(corriente_a: float, largo_m: float, seccion_mm2: float, tension_v: float = 220, fases: int = 1,
              material: str = "cobre", tipo: str = "subalimentador") -> Resultado:
    """Revisa un conductor ya elegido."""
    v = caida(corriente_a, largo_m, seccion_mm2, fases, material)
    pct = v / tension_v * 100
    lim = LIMITE.get(tipo, 3.0)
    avisos = []
    if seccion_mm2 < SECCION_MIN.get(tipo, 0):
        avisos.append(f"Bajo el mínimo del RIC N°03 para {tipo} ({SECCION_MIN[tipo]:g} mm²).")
    return Resultado(tension_v, fases, corriente_a, largo_m, material, tipo, seccion_mm2, round(v, 2), round(pct, 2), lim,
                     pct <= lim and not avisos, next((p for p in PROTECCIONES if p >= corriente_a), None), None, avisos)


def desde_frase(texto: str) -> Resultado | None:
    """«qué cable para 20 amperes a 35 metros trifásico» / «4500 W a 40 m en 220» → cálculo."""
    import re
    t = texto.lower().replace(",", ".")
    num = lambda pat: (float(m.group(1)) if (m := re.search(pat, t)) else None)  # noqa: E731
    i = num(r"(\d+(?:\.\d+)?)\s*(?:a\b|amp)")
    w = num(r"(\d+(?:\.\d+)?)\s*(?:w\b|watts?|vatios)")
    kw = num(r"(\d+(?:\.\d+)?)\s*kw")
    largo = num(r"(\d+(?:\.\d+)?)\s*(?:m\b|metros?)")
    tri = bool(re.search(r"trif|380|400\s*v", t))
    v = num(r"(\d{3})\s*v") or (380 if tri else 220)
    if i is None and (w or kw):
        i = corriente((w or 0) + (kw or 0) * 1000, v, 3 if tri else 1)
    if i is None or largo is None:
        return None
    tipo = "alimentador" if "alimentador" in t and "sub" not in t else "subalimentador"
    return seccion(i, largo, v, 3 if tri else 1, "aluminio" if "alumin" in t else "cobre", tipo)
