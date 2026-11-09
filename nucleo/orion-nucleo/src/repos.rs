//! Buscador de repositorios (antes orion_repos): antes de construir algo, la IA busca si ya existe.
//! Consulta GitHub con gh (la sesión de la persona), filtra los archivados, puntúa por estrellas, actividad y
//! licencia, y marca si sirve para uso comercial.

use crate::util::{correr, existe_programa};
use serde_json::{json, Value};

const LIBRES: [&str; 9] = ["mit", "apache-2.0", "bsd-2-clause", "bsd-3-clause", "isc", "mpl-2.0", "unlicense", "zlib", "0bsd"];
const COPYLEFT: [&str; 5] = ["gpl-2.0", "gpl-3.0", "agpl-3.0", "lgpl-2.1", "lgpl-3.0"];

fn dias_desde(iso: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(iso).map(|t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_days()).unwrap_or(9999)
}

pub fn comercial(lic: &str) -> &'static str {
    if LIBRES.contains(&lic) { "sí" } else if COPYLEFT.contains(&lic) { "sí, compartiendo el código" } else { "revisar" }
}

pub fn puntaje(estrellas: u64, dias: i64, lic: &str) -> f64 {
    let p = ((estrellas + 1) as f64).log10() * 10.0 - dias.min(720) as f64 / 72.0 + if LIBRES.contains(&lic) { 3.0 } else { 0.0 } - if lic == "sin-licencia" { 8.0 } else { 0.0 };
    (p * 10.0).round() / 10.0
}

/// Convierte la salida de `gh search repos --json …` en la lista ordenada.
pub fn ordenar(gh_json: &str, n: usize) -> Vec<Value> {
    let lista: Vec<Value> = serde_json::from_str(gh_json).unwrap_or_default();
    let mut res: Vec<Value> = lista
        .iter()
        .filter(|r| r["isArchived"] != true)
        .map(|r| {
            let lic = r["license"]["key"].as_str().filter(|s| !s.is_empty()).unwrap_or("sin-licencia");
            let dias = dias_desde(r["pushedAt"].as_str().unwrap_or(""));
            let est = r["stargazersCount"].as_u64().unwrap_or(0);
            json!({"repo": r["fullName"], "estrellas": est, "licencia": lic, "comercial": comercial(lic), "ultimo_cambio_dias": dias,
                   "lenguaje": r["language"], "que_es": r["description"].as_str().unwrap_or("").chars().take(160).collect::<String>(),
                   "url": r["url"], "puntaje": puntaje(est, dias, lic)})
        })
        .collect();
    res.sort_by(|a, b| b["puntaje"].as_f64().partial_cmp(&a["puntaje"].as_f64()).unwrap());
    res.truncate(n);
    res
}

pub fn buscar(consulta: &str, n: usize) -> Result<Vec<Value>, String> {
    if !existe_programa("gh") {
        return Err("falta gh (GitHub CLI): sudo apt install gh && gh auth login".into());
    }
    let lim = (n * 2).to_string();
    let o = correr(&["gh", "search", "repos", consulta, "--sort", "stars", "--limit", &lim, "--json", "fullName,description,stargazersCount,license,pushedAt,isArchived,url,language"]);
    if o.0 != 0 {
        return Err(o.2.trim().chars().take(300).collect());
    }
    Ok(ordenar(if o.1.trim().is_empty() { "[]" } else { &o.1 }, n))
}

/// Confirma que repositorios conocidos ('dueño/nombre') existen y trae estrellas, licencia y actividad.
pub fn verificar(repos: &[String]) -> Vec<Value> {
    let mut out = vec![];
    for (i, grupo) in repos.chunks(5).enumerate() {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(2200)); // límite de búsquedas de GitHub
        }
        let q = grupo.iter().map(|r| format!("repo:{r}")).collect::<Vec<_>>().join(" ");
        let o = correr(&["gh", "api", "-X", "GET", "search/repositories", "-f", &format!("q={q}"), "-f", "per_page=10"]);
        if o.0 != 0 {
            continue;
        }
        let v: Value = serde_json::from_str(&o.1).unwrap_or_default();
        for j in v["items"].as_array().into_iter().flatten().filter(|j| j["archived"] != true) {
            let lic = j["license"]["key"].as_str().unwrap_or("sin-licencia");
            out.push(json!({"repo": j["full_name"], "estrellas": j["stargazers_count"], "licencia": lic, "comercial": comercial(lic),
                            "ultimo_cambio_dias": dias_desde(j["pushed_at"].as_str().unwrap_or("")), "lenguaje": j["language"],
                            "que_es": j["description"].as_str().unwrap_or("").chars().take(160).collect::<String>(), "url": j["html_url"],
                            "puntaje": 100, "referente": true}));
        }
    }
    out
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn puntua_como_python() {
        let hoy = chrono::Utc::now().to_rfc3339();
        let gh = json!([
            {"fullName": "a/viejo", "stargazersCount": 50000, "license": {"key": "mit"}, "pushedAt": "2019-01-01T00:00:00Z", "isArchived": false, "url": "u", "description": "x"},
            {"fullName": "b/archivado", "stargazersCount": 90000, "license": {"key": "mit"}, "pushedAt": hoy, "isArchived": true},
            {"fullName": "c/nuevo", "stargazersCount": 900, "license": null, "pushedAt": hoy, "isArchived": false},
            {"fullName": "d/gpl", "stargazersCount": 5000, "license": {"key": "gpl-3.0"}, "pushedAt": hoy, "isArchived": false},
        ]);
        let r = ordenar(&gh.to_string(), 8);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0]["repo"], "a/viejo"); // 40 puntos contra 37 del GPL nuevo, igual que en Python
        assert_eq!(r[1]["repo"], "d/gpl");
        assert_eq!(r[1]["comercial"], "sí, compartiendo el código");
        assert_eq!(r.iter().find(|x| x["repo"] == "c/nuevo").unwrap()["licencia"], "sin-licencia");
        // log10(50001)*10 - 720/72 + 3 = 47.0 - 10 + 3
        assert_eq!(puntaje(50000, 2000, "mit"), 40.0);
    }
}
