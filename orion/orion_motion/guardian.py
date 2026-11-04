"""El Guardián: la capa que protege a la máquina (y a las personas).

Corre en el propio robot, es código fijo y NO es una IA. Revisa cada orden
antes de que llegue a los motores:
  · límites de cada articulación (recorta al rango calibrado)
  · velocidad máxima de cada servo (también el pico de las ondas)
  · el efector nunca baja del piso ni entra a zonas prohibidas
  · la base con ruedas frena antes de chocar (sensor de distancia)

Regla de diseño: la IA puede PEDIR saltarse una protección, pero no puede
AUTORIZARLO. Solo una persona confirma (por voz o botón) y solo una vez.
El paro de emergencia físico no pasa por software: corta la alimentación.
"""
from __future__ import annotations
from dataclasses import dataclass, field
from .robot import Robot
from .translator import Command
from .wave import Wave


@dataclass
class Verdict:
    ok: bool
    command: Command
    notes: list[str] = field(default_factory=list)
    needs_human: bool = False

    def __str__(self):
        tag = "OK " if self.ok else ("CONFIRMAR" if self.needs_human else "BLOQUEADO")
        return f"[{tag}] {self.command}" + ("" if not self.notes else "  · " + " · ".join(self.notes))


class Guardian:
    def __init__(self, robot: Robot, distance_sensor=None):
        self.robot = robot
        self.read_distance = distance_sensor or (lambda: None)   # mm o None
        self._human_override: str | None = None

    def human_confirm(self, rule: str):
        """Llamado SOLO desde la interfaz de la persona (botón o voz verificada)."""
        self._human_override = rule

    def _consume_override(self, rule: str) -> bool:
        if self._human_override == rule:
            self._human_override = None
            return True
        return False

    def check(self, cmd: Command) -> Verdict:
        r, s = self.robot, self.robot.seguridad
        v = Verdict(True, cmd)
        if cmd.kind in ("STOP", "HOME", "SAVE", "MACRO", "CALIBRATE"):
            return v
        if cmd.kind in ("SET", "MOVE_REL"):
            j = r.joint(cmd.joint)
            target = cmd.value if cmd.kind == "SET" else r.pose[j.id] + cmd.value
            if target < j.min or target > j.max:
                clipped = max(j.min, min(j.max, target))
                v.notes.append(f"{j.id} {target:g}° fuera de rango [{j.min},{j.max}] → recortado a {clipped:g}°")
                target = clipped
            vel = cmd.params.get("vel", j.vel_max)
            if vel > j.vel_max:
                v.notes.append(f"velocidad {vel:g}°/s > máx {j.vel_max:g} → limitada")
                vel = j.vel_max
            pose = dict(r.pose, **{j.id: target})
            ok, why = self._pose_safe(pose)
            if not ok:
                if self._consume_override("zona"):
                    v.notes.append(f"{why} — AUTORIZADO por la persona")
                else:
                    return Verdict(False, cmd, v.notes + [why], needs_human=True)
            cmd.kind, cmd.value, cmd.params = "SET", target, dict(cmd.params, vel=vel)
            return v
        if cmd.kind == "WAVE":
            j = r.joint(cmd.joint)
            p = cmd.params
            w = Wave(j.id, p["amp"], p["freq"], r.pose[j.id], 0, p["dur"], p["mult"])
            if w.center - abs(w.amplitude) < j.min or w.center + abs(w.amplitude) > j.max:
                room = min(w.center - j.min, j.max - w.center)
                v.notes.append(f"amplitud {w.amplitude:g}° excede el rango → {room:g}°")
                p["amp"] = w.amplitude = room
            if w.peak_velocity > j.vel_max:
                fmax = w.max_frequency_for(j.vel_max)
                v.notes.append(f"pico {w.peak_velocity:.0f}°/s > {j.vel_max:g}°/s → frecuencia efectiva {fmax:.2f} Hz")
                p["mult"] = fmax / p["freq"]
            for tt in (0.25 / w.f, 0.75 / w.f):            # extremos de la onda
                ok, why = self._pose_safe(dict(r.pose, **{j.id: w.angle(tt)}))
                if not ok:
                    return Verdict(False, cmd, v.notes + [why], needs_human=True)
            return v
        if cmd.kind == "DRIVE":
            dist = self.read_distance()
            brake = s.get("distancia_frenado_mm", 150)
            if dist is not None and cmd.value > 0 and cmd.value > dist - brake:
                if self._consume_override("choque"):
                    v.notes.append("avance hasta contacto AUTORIZADO por la persona")
                    return v
                allowed = max(0.0, dist - brake)
                v.notes.append(f"obstáculo a {dist:g} mm → avance limitado a {allowed:g} mm")
                cmd.value = allowed
            return v
        return v

    def _pose_safe(self, pose) -> tuple[bool, str]:
        s = self.robot.seguridad
        pts = self.robot.fk(pose)
        floor = s.get("altura_minima_mm", 0)
        for i, (x, y, z) in enumerate(pts[1:], 1):
            if z < floor:
                return False, f"punto {i} bajaría a z={z:.0f} mm (< {floor} mm del piso)"
            for zona in s.get("zonas_prohibidas", []):
                if zona["x"][0] <= x <= zona["x"][1] and zona["y"][0] <= y <= zona["y"][1] and zona["z"][0] <= z <= zona["z"][1]:
                    return False, f"punto {i} entraría a la zona prohibida '{zona['nombre']}'"
        return True, ""
