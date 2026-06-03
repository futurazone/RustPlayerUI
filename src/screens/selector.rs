//! Controlador de la pantalla Selector (carrusel de álbumes/playlists).
//!
//! Gestiona los eventos touch-up del selector:
//! - Tap en la portada central → encola las pistas y navega al Player.
//! - Tap en portada lateral → snap magnético hacia ese álbum.
//! - Drag con inercia → calcula la posición de snap al soltar.
//! - Swipe vertical hacia abajo → alterna entre modo Álbumes y Playlists.
//!
//! La elección de qué pistas enviar al backend depende de si está pausado
//! (en cuyo caso se manda /pause para hacer unpause) o parado/reproduciendo.
use crate::api;
use crate::app::state::AppState;
use crate::config::*;
use crate::AppWindow;
use slint::Model;

pub fn handle_touch_up(
    state: &AppState,
    ui: &AppWindow,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    drag: bool,
    fired: bool,
    start_off_x: f32,
    shutdown_was_visible: bool,
) {
    let (s_offset_x, s_spacing) = {
        let Ok(s) = state.interaction.swiper.try_borrow() else {
            return;
        };
        (s.offset_x, s.spacing)
    };
    let offset_diff = (s_offset_x - start_off_x).abs();

    if !drag && !fired && offset_diff < TAP_OFFSET_THRESHOLD {
        // --- SHUTDOWN: DOS PASOS ---
        // 1er toque: muestra el botón (se hace en handle_touch_down)
        // 2º toque: si el botón YA estaba visible y tocas en la zona amplia → apagar
        if shutdown_was_visible && x >= SHUTDOWN_HIT_X && y >= SHUTDOWN_HIT_Y {
            log::warn!("SHUTDOWN: Segundo toque en zona amplia (x={:.0}, y={:.0})", x, y);
            ui.invoke_shutdown();
            ui.set_shutdown_visible(false);
            return;
        }

        // TAP! (álbum central o lateral)
        let cx = CENTER_X;
        let slot = ((x - (cx + s_offset_x)) / s_spacing).round() as i32;

        if y >= ALBUM_TAP_Y_MIN && y <= ALBUM_TAP_Y_MAX {
            if slot == 0 {
                // Reproducción del elemento CENTRAL
                let mode = *state.library.current_mode.borrow();
                let target_idx = state.interaction.swiper.borrow().lib_offset + CENTER_INDEX;
                let api = state.api_url.clone();

                if let Some(item_data) = state.library.model.row_data(CENTER_INDEX as usize) {
                    log::info!("Player: Navigation to Center Item ({:?})", mode);
                    // Mostrar portada y metadatos inmediatamente desde la caché local,
                    // sin esperar el próximo status poll del backend (~1s).
                    ui.set_album_title(item_data.title.clone());
                    ui.set_album_artist(item_data.artist.clone());
                    ui.set_bg_cover(item_data.cover.clone());
                    ui.set_player_cover(item_data.cover.clone());
                    ui.set_current_track_title(item_data.title.clone());
                    ui.set_current_track_artist(item_data.artist.clone());
                    ui.set_current_screen(crate::ScreenState::Player);
                    ui.set_is_playing(true);
                }

                let albums = state.library.albums.borrow().clone();
                let playlists = state.library.playlists.borrow().clone();
                if mode == api::BrowserMode::Albums {
                    if target_idx >= 0 && (target_idx as usize) < albums.len() {
                        if let Some(tracks) = &albums[target_idx as usize].tracks {
                            let track_ids: Vec<String> = tracks.iter().map(|t| t.track_id.clone()).collect();
                            std::thread::spawn(move || {
                                let _ = api::send_queue(&api, track_ids);
                                let _ = api::send_player_command_get(&api, "play");
                            });
                        }
                    }
                } else {
                    // Modo Playlists: Fetch y Play (Estilo Python)
                    if target_idx >= 0 && (target_idx as usize) < playlists.len() {
                        if let Some(id) = playlists[target_idx as usize].id.clone() {
                            std::thread::spawn(move || {
                                if let Ok(track_ids) = api::fetch_playlist_tracks(&api, &id) {
                                    let _ = api::send_queue(&api, track_ids);
                                    let _ = api::send_player_command_get(&api, "play");
                                }
                            });
                        }
                    }
                }
            } else {
                log::info!("Interaccion: TAP Portada Lateral ({}) -> Snapping", slot);
                if let Ok(mut mut_s) = state.interaction.swiper.try_borrow_mut() {
                    // Tap lateral: avanzar un único slot en la dirección del toque
                    // para que centrar la siguiente portada sea consistente.
                    let step = if slot > 0 { 1.0 } else { -1.0 };
                    let target_snap = step * s_spacing;
                    mut_s.snap_target = mut_s.offset_x + target_snap;
                    mut_s.is_moving = true;
                    mut_s.velocity = 0.0;
                }
            }
        }
    } else if drag {
        if let Ok(mut s) = state.interaction.swiper.try_borrow_mut() {
            let vel = s.velocity;
            let off = s.offset_x;
            log::info!(
                "Selector release: dx={:.1} dy={:.1} off={:.1} vel={:.1}",
                dx,
                dy,
                off,
                vel
            );
            s.set_snap_slot(off, vel);
        }
    }

    if dy > MODE_SWIPE_DY_MIN && dx.abs() < MODE_SWIPE_DX_MAX {
        log::info!("Interaccion: SWIPE DOWN -> Toggle Browser Mode");
        ui.invoke_toggle_browser_mode();
    }
}
