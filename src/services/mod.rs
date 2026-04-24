//! Servicios de fondo del PiPlayer.
//!
//! Cada servicio corre en un hilo independiente y se comunica con el hilo
//! principal (timer de Slint) a través de canales `mpsc`:
//!
//! - `library`   → Espera en bucle hasta que el backend responde, luego carga
//!                  álbumes y playlists y los envía una única vez al canal.
//! - `playback`  → Hace polling a `/status` cada 1s y reenvía el estado al
//!                  canal para que el timer actualice la UI.
//!
//! `ServiceChannels` agrega los receivers para que `main.rs` los consuma.
pub mod playback;
pub mod library;

use std::sync::mpsc;
use crate::api;

pub struct ServiceChannels {
    pub lib_rx: mpsc::Receiver<(Vec<api::Album>, Vec<api::Playlist>)>,
    pub status_rx: mpsc::Receiver<api::PlayerStatus>,
}

pub fn spawn_all_services(api_url: &str) -> ServiceChannels {
    let (lib_tx, lib_rx) = mpsc::channel();
    let (status_tx, status_rx) = mpsc::channel();

    library::spawn_library_loader(api_url, lib_tx);
    playback::spawn_status_poller(api_url, status_tx);

    ServiceChannels {
        lib_rx,
        status_rx,
    }
}
