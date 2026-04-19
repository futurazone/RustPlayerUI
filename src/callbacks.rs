//! Registro de callbacks de UI (todo excepto touch globales).
//!
//! Conecta las acciones de Slint con la lógica de negocio:
//! - Navegación: back_to_selector, close_track_picker, album_clicked, track_clicked
//! - Browser: toggle_browser_mode (cambia entre Albums y Playlists, resetea swiper)
//! - Player: toggle_pause, play_next, play_prev, toggle_shuffle, toggle_repeat
//!   (los toggles usan estado optimista con lock de 2s antes de sincronizar con servidor)

use std::time::Instant;

use slint::{ComponentHandle, Model};

use crate::api;
use crate::app_state::AppState;
use crate::ui_utils::{get_item_slint, go_to_selector};
use crate::{AppWindow, BrowserMode, ScreenState};

/// Registra todos los callbacks de UI (excepto touch globales).
pub fn register_callbacks(ui: &AppWindow, state: &AppState) {
    // back_to_selector
    {
        let ui_weak = ui.as_weak();
        ui.on_back_to_selector(move || {
            log::info!("Gesture: back to selector");
            if let Some(ui) = ui_weak.upgrade() {
                go_to_selector(&ui);
            }
        });
    }

    // close_track_picker
    {
        let ui_weak = ui.as_weak();
        ui.on_close_track_picker(move || {
            log::info!("Modal: close track picker");
            if let Some(ui) = ui_weak.upgrade() {
                if ui.get_current_screen() == ScreenState::TrackPicker {
                    go_to_selector(&ui);
                }
            }
        });
    }

    // track_clicked
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_track_clicked(move |track_id| {
            log::info!("Track clicked: id={}", track_id);
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_current_screen(ScreenState::Player);
                let api = state.api_url.clone();
                let tid = track_id.to_string();
                std::thread::spawn(move || {
                    let _ = api::play_track(&api, &tid);
                });
            }
        });
    }

    // toggle_browser_mode
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_toggle_browser_mode(move || {
            let new_mode = {
                let mut mode = state.current_mode.borrow_mut();
                if *mode == api::BrowserMode::Albums {
                    *mode = api::BrowserMode::Playlists;
                } else {
                    *mode = api::BrowserMode::Albums;
                }
                *mode
            };
            log::info!(
                "Gesture: toggle browser mode (new={:?})",
                new_mode == api::BrowserMode::Albums
            );

            if let Some(ui) = ui_weak.upgrade() {
                ui.set_browser_mode(if new_mode == api::BrowserMode::Albums {
                    BrowserMode::Albums
                } else {
                    BrowserMode::Playlists
                });

                let lib_off = {
                    let mut s = state.swiper.borrow_mut();

                    // PERSISTENCIA: Guardamos posición actual y recuperamos la del nuevo modo
                    if new_mode == api::BrowserMode::Playlists {
                        // Veníamos de Albums -> Guardamos en albums_pos
                        *state.albums_pos.borrow_mut() = (s.offset_x, s.lib_offset);
                        let (off, loff) = *state.playlists_pos.borrow();
                        s.offset_x = off;
                        s.lib_offset = loff;
                    } else {
                        // Veníamos de Playlists -> Guardamos en playlists_pos
                        *state.playlists_pos.borrow_mut() = (s.offset_x, s.lib_offset);
                        let (off, loff) = *state.albums_pos.borrow();
                        s.offset_x = off;
                        s.lib_offset = loff;
                    }

                    s.snap_target = s.offset_x; // Snap al mismo sitio donde estábamos
                    s.velocity = 0.0;
                    s.lib_offset
                };

                let mut img_s = state.image_state.borrow_mut();
                let albums = state.albums.borrow();
                let playlists = state.playlists.borrow();
                for i in 0..7 {
                    state.model.set_row_data(
                        i,
                        get_item_slint(
                            &new_mode,
                            &albums,
                            &playlists,
                            &mut img_s,
                            &state.img_tx,
                            lib_off + i as i32,
                        ),
                    );
                }

                if let Some(item_data) = state.model.row_data(3) {
                    ui.set_bg_cover(item_data.cover.clone());
                }

                // PRECARGA: Disparamos la carga del vecindario inmediatamente al cambiar de modo
                crate::ui_utils::preload_neighborhood(
                    &new_mode,
                    &albums,
                    &playlists,
                    &mut img_s,
                    &state.img_tx,
                    lib_off,
                );
            }
        });
    }

    // album_clicked
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_album_clicked(move |visual_idx| {
            if let Some(ui) = ui_weak.upgrade() {
                if let Some(item_data) = state.model.row_data(visual_idx as usize) {
                    log::info!("Navigation: Go to Player (Click visual_idx={})", visual_idx);
                    ui.set_album_title(item_data.title.clone());
                    ui.set_album_artist(item_data.artist.clone());
                    ui.set_bg_cover(item_data.cover.clone());
                    ui.set_current_screen(ScreenState::Player);

                    // Trigger playback
                    let albums = state.albums.borrow().clone();
                    let playlists = state.playlists.borrow().clone();
                    let s = state.swiper.borrow();
                    let mode = *state.current_mode.borrow();
                    let target_idx = s.lib_offset + visual_idx as i32;
                    let api = state.api_url.clone();

                    ui.set_is_playing(true);
                    *state.playback_state.borrow_mut() = "play".to_string();

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
                }
            }
        });
    }

    // Player actions
    register_player_actions(ui, state);
}

fn register_player_actions(ui: &AppWindow, state: &AppState) {
    {
        let api_url = state.api_url.clone();
        ui.on_toggle_pause(move || {
            let api = api_url.clone();
            std::thread::spawn(move || {
                let _ = api::send_player_command_get(&api, "pause");
            });
        });
    }
    {
        let api_url = state.api_url.clone();
        ui.on_play_next(move || {
            let api = api_url.clone();
            std::thread::spawn(move || {
                let _ = api::send_player_command_get(&api, "next");
            });
        });
    }
    {
        let api_url = state.api_url.clone();
        ui.on_play_prev(move || {
            let api = api_url.clone();
            std::thread::spawn(move || {
                let _ = api::send_player_command_get(&api, "prev");
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_toggle_shuffle(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let mut opt = state.opt_shuffle.borrow_mut();
                *opt = !*opt;
                ui.set_shuffle_on(*opt);
                *state.opt_lock.borrow_mut() = Instant::now();

                let api = state.api_url.clone();
                std::thread::spawn(move || {
                    let _ = api::send_player_command_post(&api, "shuffle");
                });
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_toggle_repeat(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let mut opt = state.opt_repeat.borrow_mut();
                *opt = !*opt;
                ui.set_repeat_on(*opt);
                *state.opt_lock.borrow_mut() = Instant::now();

                let api = state.api_url.clone();
                std::thread::spawn(move || {
                    let _ = api::send_player_command_post(&api, "repeat");
                });
            }
        });
    }
}
