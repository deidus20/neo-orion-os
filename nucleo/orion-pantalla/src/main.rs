//! Neo Orión · pantalla a pantalla completa para el HUD y el instalador (antes un guion Python con GTK).
//!
//! Un navegador WebKit sin barras ni menús (GTK 3 + WebKit2GTK 4.1, paquetes oficiales de Ubuntu). Corre dentro de
//! cage (compositor Wayland de una sola ventana). Solo abre 127.0.0.1: cualquier enlace externo se manda al
//! navegador normal (Chrome/Firefox) en vez de cargarse aquí. Micrófono, cámara y avisos se permiten solo a la
//! interfaz local (registro de voz, chat de voz).
//!
//!   orion-pantalla [url]            (por defecto http://127.0.0.1:7700/)

use gtk::prelude::*;
use webkit2gtk::{
    HardwareAccelerationPolicy, NavigationPolicyDecision, NotificationPermissionRequest, PermissionRequest, PermissionRequestExt, PolicyDecisionExt, PolicyDecisionType, SettingsExt,
    NavigationPolicyDecisionExt, URIRequestExt, UserMediaPermissionRequest, WebView, WebViewExt,
};

const LOCAL: [&str; 2] = ["http://127.0.0.1:", "http://localhost:"];

/// ¿Se carga dentro de la pantalla (interfaz local) o se manda al navegador normal?
pub fn es_local(uri: &str) -> bool {
    LOCAL.iter().any(|l| uri.starts_with(l)) || uri.starts_with("about:")
}

fn al_navegador(uri: &str) {
    for b in ["google-chrome-stable", "firefox", "xdg-open"] {
        if std::process::Command::new(b).arg(uri).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().is_ok() {
            break;
        }
    }
}

fn main() {
    let url = std::env::args().nth(1).unwrap_or_else(|| "http://127.0.0.1:7700/".into());
    if gtk::init().is_err() {
        eprintln!("orion-pantalla: no hay pantalla (¿corre dentro de cage?)");
        std::process::exit(1);
    }
    let vista = WebView::new();
    if let Some(a) = WebViewExt::settings(&vista) {
        a.set_enable_developer_extras(std::env::var_os("ORION_DEPURAR").is_some());
        a.set_enable_media_stream(true);
        a.set_enable_mediasource(true);
        a.set_enable_webaudio(true);
        a.set_enable_webgl(true);
        a.set_enable_smooth_scrolling(false);
        a.set_media_playback_requires_user_gesture(false);
        a.set_enable_write_console_messages_to_stdout(true);
        a.set_hardware_acceleration_policy(if std::env::var_os("LIBGL_ALWAYS_SOFTWARE").is_some() { HardwareAccelerationPolicy::Never } else { HardwareAccelerationPolicy::OnDemand });
    }
    vista.connect_permission_request(|_, p: &PermissionRequest| {
        if p.is::<UserMediaPermissionRequest>() || p.is::<NotificationPermissionRequest>() {
            p.allow();
        } else {
            p.deny();
        }
        true
    });
    vista.connect_decide_policy(|_, d, tipo| {
        if matches!(tipo, PolicyDecisionType::NavigationAction | PolicyDecisionType::NewWindowAction) {
            if let Some(nav) = d.downcast_ref::<NavigationPolicyDecision>() {
                let uri = nav.navigation_action().and_then(|a| a.request()).and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
                if !uri.is_empty() && !es_local(&uri) {
                    d.ignore();
                    al_navegador(&uri);
                    return true;
                }
            }
        }
        false
    });
    vista.connect_web_process_terminated(|v, _| v.reload()); // si el motor se cae, vuelve solo
    let ventana = gtk::Window::new(gtk::WindowType::Toplevel);
    ventana.set_title("Neo Orión");
    ventana.set_default_size(1280, 800);
    ventana.set_decorated(false); // sin barra de título: la interfaz es toda la pantalla
    ventana.add(&vista);
    ventana.connect_destroy(|_| gtk::main_quit());
    ventana.fullscreen();
    ventana.show_all();
    vista.load_uri(&url);
    gtk::main();
}

#[cfg(test)]
mod pruebas {
    #[test]
    fn solo_lo_local_se_queda() {
        assert!(super::es_local("http://127.0.0.1:7700/hud/"));
        assert!(super::es_local("http://localhost:7743/"));
        assert!(super::es_local("about:blank"));
        assert!(!super::es_local("https://claude.ai/"));
        assert!(!super::es_local("http://127.0.0.2:7700/"));
    }
}
