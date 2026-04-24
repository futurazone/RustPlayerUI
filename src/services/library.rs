//! Servicio de carga de biblioteca.
//!
//! Ejecuta un bucle de reintento cada 500ms hasta que el backend de Python
//! responde con datos válidos. Una vez obtenidos, envía álbumes y playlists
//! al canal `lib_tx` y termina. Este diseño garantiza que la Splash Screen
//! se mantenga visible hasta que el backend esté completamente listo.
use std::sync::mpsc;
use std::time::Duration;
use crate::api;


pub fn spawn_library_loader(api_url: &str, lib_tx: mpsc::Sender<(Vec<api::Album>, Vec<api::Playlist>)>) {
    let api_url = api_url.to_string();
    std::thread::spawn(move || {
        loop {
            match api::fetch_real_albums(&api_url) {
                Ok(albums) => {
                    log::info!("LibraryService: Conexión con backend exitosa. Cargando playlists...");
                    let playlists = api::fetch_real_playlists(&api_url).unwrap_or_default();
                    let _ = lib_tx.send((albums, playlists));
                    break;
                }
                Err(e) => {
                    log::info!("Splash: Esperando al backend... ({})", e);
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    });
}
