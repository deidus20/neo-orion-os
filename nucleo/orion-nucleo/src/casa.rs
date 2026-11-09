//! Casa inteligente por voz con Home Assistant: «apaga la luz del taller», «¿qué temperatura hay en la pieza?».
//! El token de acceso lo crea la persona en Home Assistant y lo deja en ~/.orion/claves/home-assistant (600).
//! Cerraduras, alarmas, portones y garajes son acciones FÍSICAS: piden confirmación.

use crate::util::{parecido, sin_tilde};
use regex::Regex;
use serde_json::{json, Value};

const SENSIBLES: [&str; 5] = ["lock.", "alarm_control_panel.", "cover.garage", "cover.porton", "cover.gate"];
const ACCIONES: [(&str, &str); 7] = [
    (r"\b(apaga|apagar|desactiva|cierra la llave de)\b", "turn_off"),
    (r"\b(enciende|encender|prende|prender|activa|abre la llave de)\b", "turn_on"),
    (r"\b(alterna|cambia)\b", "toggle"),
    (r"\b(desbloquea|abre la cerradura)\b", "unlock"),
    (r"\b(bloquea|cierra con llave|cierra la cerradura)\b", "lock"),
    (r"\b(abre|sube)\b", "open_cover"),
    (r"\b(cierra|baja)\b", "close_cover"),
];

pub struct Casa {
    pub url: String,
    pub token: String,
    /// Para pruebas: estados fijos en vez de pedirlos a Home Assistant; y registro de servicios llamados.
    pub estados_fijos: Option<Vec<Value>>,
    pub llamados: std::cell::RefCell<Vec<(String, Value)>>,
}

impl Casa {
    pub fn cargar() -> Self {
        let token = std::fs::read_to_string(crate::util::casa().join(".orion/claves/home-assistant")).map(|s| s.trim().to_string()).unwrap_or_default();
        Casa { url: std::env::var("ORION_HA_URL").unwrap_or_else(|_| "http://homeassistant.local:8123".into()), token, estados_fijos: None, llamados: Default::default() }
    }

    fn pedir(&self, metodo: &str, ruta: &str, datos: Option<Value>) -> Result<Value, String> {
        if self.token.is_empty() {
            return Err("Falta el token de Home Assistant: créalo en tu perfil de HA y guárdalo en ~/.orion/claves/home-assistant".into());
        }
        if self.estados_fijos.is_some() {
            self.llamados.borrow_mut().push((ruta.into(), datos.unwrap_or(Value::Null)));
            return Ok(Value::Null);
        }
        let r = ureq::request(metodo, &format!("{}{}", self.url.trim_end_matches('/'), ruta))
            .set("Authorization", &format!("Bearer {}", self.token))
            .timeout(std::time::Duration::from_secs(10));
        let r = match datos {
            Some(d) => r.send_json(d),
            None => r.call(),
        }
        .map_err(|e| e.to_string())?;
        r.into_json().map_err(|e| e.to_string())
    }

    pub fn estados(&self) -> Result<Vec<Value>, String> {
        if self.token.is_empty() {
            return Err("Falta el token de Home Assistant: créalo en tu perfil de HA y guárdalo en ~/.orion/claves/home-assistant".into());
        }
        if let Some(e) = &self.estados_fijos {
            return Ok(e.clone());
        }
        Ok(self.pedir("GET", "/api/states", None)?.as_array().cloned().unwrap_or_default())
    }

    /// La entidad cuyo nombre se parece más a lo dicho («luz del taller» → light.taller).
    pub fn buscar(&self, texto: &str, dominios: &[&str]) -> Result<Option<Value>, String> {
        let limpio = Regex::new(r"\b(apaga|enciende|prende|abre|cierra|sube|baja|la|el|los|las|del|de|al|a)\b").unwrap().replace_all(&sin_tilde(texto), " ").to_string();
        let objetivo = limpio.split_whitespace().collect::<Vec<_>>().join(" ");
        let (mut mejor, mut puntaje) = (None, 0.0);
        for e in self.estados()? {
            let id = e["entity_id"].as_str().unwrap_or("");
            if !dominios.is_empty() && !dominios.iter().any(|d| id.starts_with(d)) {
                continue;
            }
            let nombre = sin_tilde(&format!("{} {}", e["attributes"]["friendly_name"].as_str().unwrap_or(""), id.split_once('.').map(|x| x.1).unwrap_or("").replace('_', " ")));
            let p = parecido(&objetivo, &nombre) + 0.25 * objetivo.split_whitespace().filter(|w| w.chars().count() > 2 && nombre.contains(w)).count() as f64;
            if p > puntaje {
                (mejor, puntaje) = (Some(e), p);
            }
        }
        Ok(if puntaje >= 0.55 { mejor } else { None })
    }

    pub fn ordenar(&self, texto: &str, confirmado: bool) -> Value {
        match self.ordenar_r(texto, confirmado) {
            Ok(v) => v,
            Err(e) => json!({"ok": false, "error": e}),
        }
    }

    fn ordenar_r(&self, texto: &str, confirmado: bool) -> Result<Value, String> {
        let t = sin_tilde(texto);
        let servicio = ACCIONES.iter().find(|(p, _)| Regex::new(p).unwrap().is_match(&t)).map(|x| x.1);
        let Some(servicio) = servicio else {
            let Some(e) = self.buscar(texto, &["sensor.", "climate.", "binary_sensor."])? else { return Ok(json!({"ok": false, "error": "no entendí qué aparato"})) };
            return Ok(json!({"ok": true, "consulta": e["attributes"]["friendly_name"].as_str().or(e["entity_id"].as_str()), "valor": e["state"],
                             "unidad": e["attributes"]["unit_of_measurement"].as_str().unwrap_or("")}));
        };
        let dominios: &[&str] = match servicio {
            "open_cover" | "close_cover" => &["cover."],
            "lock" | "unlock" => &["lock."],
            _ => &["light.", "switch.", "fan.", "climate.", "media_player.", "input_boolean.", "valve."],
        };
        let Some(e) = self.buscar(texto, dominios)? else { return Ok(json!({"ok": false, "error": "no encontré ese aparato en Home Assistant"})) };
        let ent = e["entity_id"].as_str().unwrap_or("").to_string();
        if SENSIBLES.iter().any(|s| ent.starts_with(s)) && !confirmado {
            return Ok(json!({"ok": false, "necesita_confirmar": true, "entidad": ent, "servicio": servicio, "motivo": "cerraduras, alarmas y portones necesitan tu confirmación"}));
        }
        let mut dominio = ent.split('.').next().unwrap_or("").to_string();
        if ["turn_on", "turn_off", "toggle"].contains(&servicio) && !["light", "switch", "fan", "climate", "media_player", "input_boolean"].contains(&dominio.as_str()) {
            dominio = "homeassistant".into();
        }
        self.pedir("POST", &format!("/api/services/{dominio}/{servicio}"), Some(json!({"entity_id": ent})))?;
        Ok(json!({"ok": true, "entidad": ent, "nombre": e["attributes"]["friendly_name"].as_str().unwrap_or(&ent), "servicio": servicio}))
    }
}

pub const INSTALAR: [&str; 4] = [
    "sudo apt-get install -y docker.io",
    "sudo docker run -d --name homeassistant --restart=unless-stopped --privileged -e TZ=America/Santiago -v ~/.orion/home-assistant:/config -v /run/dbus:/run/dbus:ro --network=host ghcr.io/home-assistant/home-assistant:stable",
    "# abre http://localhost:8123, crea tu cuenta y luego un token en Perfil → Seguridad",
    "install -m 600 /dev/stdin ~/.orion/claves/home-assistant   # pega el token y Ctrl-D",
];

#[cfg(test)]
mod pruebas {
    use super::*;
    fn casa() -> Casa {
        Casa {
            url: "http://ha".into(),
            token: "x".into(),
            estados_fijos: Some(vec![
                json!({"entity_id": "light.taller", "state": "off", "attributes": {"friendly_name": "Luz del taller"}}),
                json!({"entity_id": "sensor.temp_pieza", "state": "21.5", "attributes": {"friendly_name": "Temperatura pieza", "unit_of_measurement": "°C"}}),
                json!({"entity_id": "lock.puerta", "state": "locked", "attributes": {"friendly_name": "Cerradura puerta"}}),
                json!({"entity_id": "switch.enchufe_cnc", "state": "on", "attributes": {"friendly_name": "Enchufe CNC"}}),
            ]),
            llamados: Default::default(),
        }
    }

    #[test]
    fn luces_consultas_y_confirmacion() {
        let c = casa();
        let r = c.ordenar("apaga la luz del taller", false);
        assert_eq!((r["ok"].as_bool(), r["entidad"].as_str()), (Some(true), Some("light.taller")));
        assert_eq!(c.llamados.borrow()[0].0, "/api/services/light/turn_off");
        assert_eq!(c.ordenar("qué temperatura hay en la pieza", false)["valor"], "21.5");
        assert_eq!(c.ordenar("desbloquea la cerradura de la puerta", false)["necesita_confirmar"], true);
        assert_eq!(c.ordenar("desbloquea la cerradura de la puerta", true)["ok"], true);
        assert_eq!(c.ordenar("enciende el enchufe de la cnc", false)["entidad"], "switch.enchufe_cnc");
        let sin = Casa { token: String::new(), ..casa() };
        assert!(sin.ordenar("apaga la luz", false)["error"].as_str().unwrap().contains("token"));
    }
}
