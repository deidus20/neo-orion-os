import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from orion_core.rendimiento import Vigia, medir, nivel_de  # noqa: E402

# Un PC bueno funcionando bien
assert nivel_de({"mem_psi": 0.5, "cpu_psi": 5, "carga": .3, "tok_s": 35, "fps": 60})[0] == "NORMAL"
# Notebook potente pero recalentado y con la IA lenta → patata aunque tenga buena ficha
n, r = nivel_de({"temp": 97, "tok_s": 2.1})
assert n == "PATATA" and "la IA responde lento" in r and "el procesador está muy caliente" in r, (n, r)
# Poca memoria libre → liviano
assert nivel_de({"mem_psi": 14})[0] == "LIVIANO"
# Batería baja y descargando → liviano
assert nivel_de({"bateria_descargando": True, "bateria": 12})[0] == "LIVIANO"

# Histéresis: baja tras 15 s malos, no oscila con un pico, sube lento y de a un nivel
v = Vigia()
malo, bueno = {"tok_s": 2, "fps": 20}, {"tok_s": 30, "fps": 60}
assert v.actualizar(malo, 0) is None and v.actualizar(malo, 10) is None
c = v.actualizar(malo, 16)
assert c and c["a"] == "PATATA" and "ia:pesado-a-la-nube" in c["acciones"] and "modo completo" in c["aviso"], c
assert v.actualizar(bueno, 20) is None and v.actualizar(malo, 25) is None   # un pico bueno no lo devuelve
assert v.actualizar(bueno, 30) is None and v.actualizar(bueno, 100) is None
c = v.actualizar(bueno, 151)
assert c and c["a"] == "LIVIANO", c                                         # sube de a un nivel
c = v.actualizar(bueno, 160); assert c is None
assert v.actualizar(bueno, 272) is None; c = v.actualizar(bueno, 281); assert c and c["a"] == "NORMAL"
# Si la persona lo fija por voz, el vigía no lo toca
v.fijo = "NORMAL"; assert v.actualizar(malo, 400) is None and v.actualizar(malo, 1000) is None

m = medir({"fps": 58})
assert "mem_psi" in m and m["fps"] == 58
print("rendimiento ok", {k: m[k] for k in ("mem_psi", "cpu_psi", "carga", "temp")})
