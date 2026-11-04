"""Cuadernos: fuentes para Gemini Notebook (ex NotebookLM) y Open Notebook local."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import orion_cuaderno as cu  # noqa: E402
import orion_navegador as nv  # noqa: E402
from orion_memoria import Memoria  # noqa: E402


def test_fuentes_desde_memoria(tmp_path, monkeypatch):
    monkeypatch.setenv("HOME", str(tmp_path))
    mem = Memoria(str(tmp_path / "Orion"), embeddings=None)
    mem.nota("Exoesqueleto", "Actuación por burbuja en V frontal y suspensión en riel de techo.")
    mem.nota("Cocina", "Receta de galletas.")
    r = cu.preparar_fuentes("exoesqueleto burbuja", mem, {"Encargo": "hazme una guía de estudio"}, raiz=tmp_path / "Orion")
    assert r["ok"]
    textos = "".join(Path(f).read_text() for f in r["fuentes"])
    assert "burbuja" in textos and "galletas" not in textos and "guía de estudio" in textos
    assert (Path(r["carpeta"]) / "00-LEEME.md").exists()


def test_gemini_notebook_se_abre_no_se_maneja(tmp_path, monkeypatch):
    monkeypatch.setenv("HOME", str(tmp_path))
    abiertas = []
    r = cu.abrir_gemini_notebook("lidar", textos={"Notas": "LiDAR 2D"}, abrir=lambda u: abiertas.append(u) or {"ok": True})
    assert abiertas == [cu.WEB] and r["modo"] == "manejas tú"
    assert not nv.automatizable("https://notebooklm.google.com/notebook/abc")
    assert nv.destino("notebooklm") == cu.WEB


def test_recibir_guarda(tmp_path):
    mem = Memoria(str(tmp_path / "Orion"), embeddings=None)
    r = cu.recibir("lidar", "Resumen: el LiDAR mide distancias.", mem)
    assert r["ok"] and "LiDAR mide" in Path(r["guardado"]).read_text()


def test_open_notebook_falso(tmp_path, monkeypatch):
    monkeypatch.setenv("HOME", str(tmp_path))
    llamadas = []
    class Falso(cu.OpenNotebook):
        def _req(self, m, ruta, datos=None):
            llamadas.append((m, ruta, datos))
            return {"id": "notebook:1"} if ruta == "/api/notebooks" and m == "POST" else []
    r = cu.enviar_open_notebook("drones", textos={"Ideas": "dron de carga"}, cliente=Falso())
    assert r["ok"] and r["fuentes"] == 1
    assert ("POST", "/api/notebooks") in [(m, x) for m, x, _ in llamadas]
    assert any(x == "/api/sources" and d["notebook_id"] == "notebook:1" for _, x, d in llamadas)


def test_open_notebook_apagado():
    class Apagado(cu.OpenNotebook):
        def disponible(self):
            return False
    r = cu.enviar_open_notebook("x", cliente=Apagado())
    assert not r["ok"] and any("docker compose" in p for p in r["instalar"])
