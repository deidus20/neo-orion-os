"""Perfil del robot y cinemática directa (Denavit-Hartenberg)."""
from __future__ import annotations
import json, math, unicodedata
from dataclasses import dataclass, field


def norm(txt: str) -> str:
    """minúsculas y sin tildes, para comparar lo que dice la IA."""
    t = unicodedata.normalize("NFD", txt.lower())
    return "".join(c for c in t if unicodedata.category(c) != "Mn")


@dataclass
class Joint:
    id: str
    alias: list[str]
    min: float
    max: float
    home: float
    vel_max: float               # grados por segundo
    dh: list[float] | None       # [a_mm, alpha_deg, d_mm, theta_offset_deg]
    offset: float = 0.0          # corrección de calibración
    direction: int = 1           # +1 / -1 si el servo está montado al revés


@dataclass
class Robot:
    nombre: str
    joints: list[Joint]
    seguridad: dict
    pose: dict[str, float] = field(default_factory=dict)

    @classmethod
    def load(cls, path: str) -> "Robot":
        d = json.load(open(path, encoding="utf-8"))
        joints = [Joint(j["id"], j["alias"], j["min"], j["max"], j["home"], j["vel_max"], j.get("dh"))
                  for j in d["articulaciones"]]
        r = cls(d["nombre"], joints, d.get("seguridad", {}))
        r.pose = {j.id: j.home for j in joints}
        return r

    def joint(self, jid: str) -> Joint:
        for j in self.joints:
            if j.id == jid:
                return j
        raise KeyError(jid)

    # ---- cinemática directa -------------------------------------------
    def fk(self, pose: dict[str, float] | None = None) -> list[tuple[float, float, float]]:
        """Devuelve la posición (mm) de cada articulación, de la base al efector."""
        pose = pose or self.pose
        T = _identity()
        points = [(0.0, 0.0, 0.0)]
        for j in self.joints:
            if not j.dh:
                continue
            a, alpha, d, off = j.dh
            theta = math.radians(pose.get(j.id, j.home) + off)
            T = _matmul(T, _dh(a, math.radians(alpha), d, theta))
            points.append((round(T[0][3], 1), round(T[1][3], 1), round(T[2][3], 1)))
        return points

    def end_effector(self, pose=None):
        return self.fk(pose)[-1]


def _dh(a, alpha, d, theta):
    ct, st, ca, sa = math.cos(theta), math.sin(theta), math.cos(alpha), math.sin(alpha)
    return [[ct, -st * ca, st * sa, a * ct],
            [st, ct * ca, -ct * sa, a * st],
            [0, sa, ca, d],
            [0, 0, 0, 1]]


def _identity():
    return [[1 if i == j else 0 for j in range(4)] for i in range(4)]


def _matmul(A, B):
    return [[sum(A[i][k] * B[k][j] for k in range(4)) for j in range(4)] for i in range(4)]
