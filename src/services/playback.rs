//! Servicio de polling del estado del reproductor.
//!
//! Hace GET `/status` al backend cada 1 segundo en un hilo dedicado.
//! El estado recibido se envía al canal `status_tx` para que el timer
//! principal actualice la UI (metadata, barra de progreso, botones,
//! y watchdog de inactividad).
use std::sync::mpsc;
use std::time::Duration;
use crate::api;

pub fn spawn_status_poller(api_url: &str, status_tx: mpsc::Sender<api::PlayerStatus>) {
    let api_url = api_url.to_string();
    std::thread::spawn(move || loop {
        if let Ok(status) = api::get_real_status(&api_url) {
            let _ = status_tx.send(status);
        }
        std::thread::sleep(Duration::from_secs(1));
    });
}
