"""Movimiento por onda: la misma trigonometría de una onda aplicada a un servo.

    θ(t) = centro + A · sin(2π · f · t + φ)

    A  = amplitud (grados): cuánto se abre el movimiento
    f  = frecuencia (Hz): cuántas oscilaciones por segundo → la VELOCIDAD
    φ  = fase (grados): desfase entre articulaciones (para caminar, ondular)

Ojo: multiplicar toda la ecuación por k agranda la AMPLITUD, no la velocidad.
Para ir más rápido se multiplica la FRECUENCIA. La velocidad angular es la
derivada:  ω(t) = A · 2πf · cos(2πft + φ)   →   pico = 2π · f · A  (°/s)
Ese pico es lo que hay que comparar con la velocidad máxima del servo.
"""
from __future__ import annotations
import math
from dataclasses import dataclass


@dataclass
class Wave:
    joint: str
    amplitude: float          # grados
    frequency: float          # Hz
    center: float = 0.0       # grados
    phase: float = 0.0        # grados
    duration: float = 4.0     # segundos
    speed_mult: float = 1.0   # multiplica la frecuencia (no la amplitud)

    @property
    def f(self) -> float:
        return self.frequency * self.speed_mult

    def angle(self, t: float) -> float:
        return self.center + self.amplitude * math.sin(2 * math.pi * self.f * t + math.radians(self.phase))

    def velocity(self, t: float) -> float:
        return self.amplitude * 2 * math.pi * self.f * math.cos(2 * math.pi * self.f * t + math.radians(self.phase))

    @property
    def peak_velocity(self) -> float:
        return abs(2 * math.pi * self.f * self.amplitude)

    def max_frequency_for(self, vel_max: float) -> float:
        """Frecuencia máxima que el servo aguanta con esta amplitud."""
        return vel_max / (2 * math.pi * abs(self.amplitude)) if self.amplitude else float("inf")

    def samples(self, rate_hz: float = 50.0):
        n = int(self.duration * rate_hz)
        return [(round(i / rate_hz, 3), round(self.angle(i / rate_hz), 2)) for i in range(n + 1)]
