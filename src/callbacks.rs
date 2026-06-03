//! Registro de callbacks de UI (todo excepto touch globales).
//!
//! Conecta las acciones de Slint con la lógica de negocio:
//! - Navegación: back_to_selector, close_track_picker, track_clicked
//! - Browser: toggle_browser_mode (cambia entre Albums y Playlists, resetea swiper)
//! - Player: toggle_pause, play_next, play_prev, toggle_shuffle, toggle_repeat
//!   (los toggles usan estado optimista con lock de 2s antes de sincronizar con servidor)

use std::time::Instant;

use slint::{ComponentHandle, Model};

use crate::api;
use crate::app::state::AppState;
use crate::config::{CENTER_INDEX, VISIBLE_SLOTS};
use crate::ui_utils::{enqueue_preload_range, get_item_slint, go_to_selector};
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

    // shutdown
    {
        let ui_weak = ui.as_weak();
        let api_url = state.api_url.clone();
        ui.on_shutdown(move || {
            log::warn!("SHUTDOWN triggered!");
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_is_shutting_down(true);
            }
            let api = api_url.clone();
            std::thread::spawn(move || {
                let _ = api::send_player_command_post(&api, "shutdown");
            });
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
                let mut mode = state.library.current_mode.borrow_mut();
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
                    let mut s = state.interaction.swiper.borrow_mut();

                    // PERSISTENCIA: Guardamos posición actual y recuperamos la del nuevo modo
                    if new_mode == api::BrowserMode::Playlists {
                        // Veníamos de Albums -> Guardamos en albums_pos
                        *state.interaction.albums_pos.borrow_mut() = (s.offset_x, s.lib_offset);
                        let (off, loff) = *state.interaction.playlists_pos.borrow();
                        s.offset_x = off;
                        s.lib_offset = loff;
                    } else {
                        // Veníamos de Playlists -> Guardamos en playlists_pos
                        *state.interaction.playlists_pos.borrow_mut() = (s.offset_x, s.lib_offset);
                        let (off, loff) = *state.interaction.albums_pos.borrow();
                        s.offset_x = off;
                        s.lib_offset = loff;
                    }

                    s.snap_target = s.offset_x; // Snap al mismo sitio donde estábamos
                    s.velocity = 0.0;
                    s.lib_offset
                };

                let mut img_s = state.library.image_state.borrow_mut();
                let albums = state.library.albums.borrow();
                let playlists = state.library.playlists.borrow();
                for i in 0..VISIBLE_SLOTS {
                    state.library.model.set_row_data(
                        i as usize,
                        get_item_slint(
                            &new_mode,
                            &albums,
                            &playlists,
                            &mut img_s,
                            &state.library.loader,
                            lib_off + i,
                        ),
                    );
                }

                if let Some(item_data) = state.library.model.row_data(CENTER_INDEX as usize) {
                    ui.set_bg_cover(item_data.cover.clone());
                }

                // PRECARGA: Reiniciamos ventana al cambiar de modo
                let window_center = lib_off + CENTER_INDEX;
                *state.library.preload_window_center.borrow_mut() = window_center;
                enqueue_preload_range(
                    window_center,
                    crate::config::PRELOAD_WINDOW_HALF,
                    &new_mode,
                    &albums,
                    &playlists,
                    &mut img_s,
                    &state.library.loader,
                );
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
                let mut opt = state.playback.opt_shuffle.borrow_mut();
                *opt = !*opt;
                ui.set_shuffle_on(*opt);
                *state.playback.opt_lock.borrow_mut() = Instant::now();

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
                let mut opt = state.playback.opt_repeat.borrow_mut();
                *opt = !*opt;
                ui.set_repeat_on(*opt);
                *state.playback.opt_lock.borrow_mut() = Instant::now();

                let api = state.api_url.clone();
                std::thread::spawn(move || {
                    let _ = api::send_player_command_post(&api, "repeat");
                });
            }
        });
    }
}
