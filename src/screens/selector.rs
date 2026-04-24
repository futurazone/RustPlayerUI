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
) {
    let (s_offset_x, s_spacing) = {
        let Ok(s) = state.interaction.swiper.try_borrow() else {
            return;
        };
        (s.offset_x, s.spacing)
    };
    let offset_diff = (s_offset_x - start_off_x).abs();

    if !drag && !fired && offset_diff < TAP_OFFSET_THRESHOLD {
        // TAP!
        let cx = CENTER_X;
        let slot = ((x - (cx + s_offset_x)) / s_spacing).round() as i32;

        if y >= ALBUM_TAP_Y_MIN && y <= ALBUM_TAP_Y_MAX {
            if slot == 0 {
                // Reproducción del elemento CENTRAL
                let mode = *state.library.current_mode.borrow();
                let target_idx = state.interaction.swiper.borrow().lib_offset + 3;
                let api = state.api_url.clone();

                if let Some(item_data) = state.library.model.row_data(3) {
                    log::info!("Player: Navigation to Center Item ({:?})", mode);
                    ui.set_album_title(item_data.title.clone());
                    ui.set_album_artist(item_data.artist.clone());
                    ui.set_bg_cover(item_data.cover.clone());
                    ui.set_current_screen(crate::ScreenState::Player);
                    ui.set_is_playing(true);
                }

                let albums = state.library.albums.borrow().clone();
                let playlists = state.library.playlists.borrow().clone();
                let is_paused = *state.playback.playback_state.borrow() == "pause";
                if mode == api::BrowserMode::Albums {
                    if target_idx >= 0 && (target_idx as usize) < albums.len() {
                        if let Some(tracks) = &albums[target_idx as usize].tracks {
                            let track_ids: Vec<String> = tracks.iter().map(|t| t.track_id.clone()).collect();
                            std::thread::spawn(move || {
                                let _ = api::send_queue(&api, track_ids);
                                if is_paused {
                                    let _ = api::send_player_command_get(&api, "pause");
                                }
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
                                    if is_paused {
                                        let _ = api::send_player_command_get(&api, "pause");
                                    }
                                }
                            });
                        }
                    }
                }
            } else {
                log::info!("Interaccion: TAP Portada Lateral ({}) -> Snapping", slot);
                if let Ok(mut mut_s) = state.interaction.swiper.try_borrow_mut() {
                    let target_snap = slot as f32 * s_spacing;
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
            s.set_snap_slot(off, vel);
        }
    }

    if dy > MODE_SWIPE_DY_MIN && dx.abs() < MODE_SWIPE_DX_MAX {
        log::info!("Interaccion: SWIPE DOWN -> Toggle Browser Mode");
        ui.invoke_toggle_browser_mode();
    }
}
