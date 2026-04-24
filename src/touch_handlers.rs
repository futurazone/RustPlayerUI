//! Gestión de touch global para las tres pantallas (Selector, Player, TrackPicker).
//!
//! Flujo: `register_touch_handlers` conecta los 3 eventos Slint (down/move/up).
//! - Down: inicia tracking, detecta zona (alphabet bar, esquinas, swiper)
//! - Move: arrastra swiper horizontal o scroll vertical, o desliza por alphabet bar
//! - Up: resuelve la acción (tap centro/lateral, swipe, botón player, pista)
//!
//! `check_long_press` se llama desde el timer tick para abrir el TrackPicker
//! al mantener pulsado sobre un álbum (~400ms).

use std::rc::Rc;
use std::time::Instant;

use slint::{ComponentHandle, Model};

use crate::api;
use crate::app::state::AppState;
use crate::config::*;
use crate::touch::transform_touch;
use crate::ui_utils::go_to_selector;
use crate::warp;
use crate::{AppWindow, ScreenState, TrackData};
use crate::screens;

/// Registra los handlers de touch globales en la UI.
pub fn register_touch_handlers(ui: &AppWindow, state: &AppState) {
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_global_touch_down(move |raw_x, raw_y| {
            handle_touch_down(&state, &ui_weak, raw_x, raw_y);
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_global_touch_move(move |raw_x, raw_y| {
            handle_touch_move(&state, &ui_weak, raw_x, raw_y);
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_global_touch_up(move |raw_x, raw_y| {
            handle_touch_up(&state, &ui_weak, raw_x, raw_y);
        });
    }
}

/// Resuelve la letra del alfabeto correspondiente a una coordenada X.
fn resolve_alphabet_char(x: f32) -> char {
    let margin = 40.0;
    let width = 1200.0; // 1280 - 40 - 40
    let calib_f = ((x - margin) / width).clamp(0.0, 1.0);
    let alphabet = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let char_idx = (calib_f * alphabet.len() as f32).floor() as usize;
    let char_idx = char_idx.min(alphabet.len() - 1);
    alphabet.chars().nth(char_idx).unwrap_or('#')
}

fn handle_touch_down(state: &AppState, ui_weak: &slint::Weak<AppWindow>, raw_x: f32, raw_y: f32) {
    *state.interaction.last_interaction.borrow_mut() = Instant::now();
    let (x, y) = transform_touch(raw_x, raw_y);
    log::info!("Touch DOWN: Raw({:.1}, {:.1}) -> Transformed({:.1}, {:.1})", raw_x, raw_y, x, y);
    let mut ts = state.interaction.touch.borrow_mut();
    let now = Instant::now();
    ts.active = true;
    ts.start_time = Some(now);
    ts.last_time = now;
    ts.start_x = x;
    ts.start_y = y;
    ts.last_x = x;
    ts.last_y = y;
    ts.is_drag = false;
    ts.long_press_fired = false;

    let mut s = state.interaction.swiper.borrow_mut();
    ts.start_offset_x = s.offset_x;
    ts.start_offset_y = state.interaction.track_physics.borrow().offset_y;

    if s.is_moving && s.velocity.abs() > 100.0 {
        s.velocity *= 0.3; // "Catch" effect from Python
    } else {
        s.velocity = 0.0;
    }

    let current_screen = ui_weak
        .upgrade()
        .map(|u| u.get_current_screen())
        .unwrap_or(ScreenState::Selector);

    if y < 90.0 && current_screen == ScreenState::Selector {
        ts.is_alphabet = true;
        let target_char = resolve_alphabet_char(x);
        let margin = 40.0;
        let width = 1200.0;
        let calib_f = ((x - margin) / width).clamp(0.0, 1.0);
        log::info!(
            "ALPHABET TOUCH (DOWN): '{}' (x={:.1}, f={:.2})",
            target_char,
            x,
            calib_f
        );

        // Trigger instant jump on Down
        if let (Ok(mut w), Ok(albs)) = (state.interaction.warp.try_borrow_mut(), state.library.albums.try_borrow()) {
            warp::trigger_warp_jump(&mut w, &albs, s.lib_offset, target_char, "DOWN");
        }
    } else if y < CORNER_TOUCH_SIZE {
        if x < CORNER_TOUCH_SIZE {
            log::info!("CORNER TOUCH: TOP-LEFT ({:.1}, {:.1})", x, y);
        } else if x > (SCREEN_WIDTH - CORNER_TOUCH_SIZE) {
            log::info!("CORNER TOUCH: TOP-RIGHT ({:.1}, {:.1})", x, y);
        }
    } else if y > (SCREEN_HEIGHT - CORNER_TOUCH_SIZE) {
        if x < CORNER_TOUCH_SIZE {
            log::info!("CORNER TOUCH: BOTTOM-LEFT ({:.1}, {:.1})", x, y);
        } else if x > (SCREEN_WIDTH - CORNER_TOUCH_SIZE) {
            log::info!("CORNER TOUCH: BOTTOM-RIGHT ({:.1}, {:.1})", x, y);
        }
    }

    if !ts.is_alphabet && s.is_moving && s.velocity.abs() > 20.0 {
        s.is_moving = false; // "Catch" the swiper
        s.velocity = 0.0;
    }
}

fn handle_touch_move(
    state: &AppState,
    ui_weak: &slint::Weak<AppWindow>,
    raw_x: f32,
    raw_y: f32,
) {
    *state.interaction.last_interaction.borrow_mut() = Instant::now();
    let now = Instant::now();
    let mut ts = state.interaction.touch.borrow_mut();
    if !ts.active {
        return;
    }
    let (x, y) = transform_touch(raw_x, raw_y);

    if !ts.is_drag {
        let dx = x - ts.start_x;
        let dy = y - ts.start_y;
        if (dx * dx + dy * dy) > DRAG_THRESHOLD_SQ {
            ts.is_drag = true;
        }
    }

    if ts.is_alphabet {
        let target_char = resolve_alphabet_char(x);

        // --- Lógica de Salto (Warp) ---
        if let (Ok(mut w), Ok(albs)) = (state.interaction.warp.try_borrow_mut(), state.library.albums.try_borrow()) {
            let lib_offset = state.interaction.swiper.borrow().lib_offset;
            warp::trigger_warp_jump(&mut w, &albs, lib_offset, target_char, "MOVE");
        }
    } else if ts.is_drag {
        if let Some(u) = ui_weak.upgrade() {
            let screen = u.get_current_screen();
            if screen == ScreenState::Selector {
                let dx = x - ts.last_x;
                let dt = ts.last_time.elapsed().as_secs_f32();

                let mut s = state.interaction.swiper.borrow_mut();
                s.is_moving = true;
                s.offset_x += dx;

                if dt > 0.001 {
                    let inst_v = dx / dt;
                    s.velocity = inst_v * 0.85 + s.velocity * 0.15; // Smoothing Python style
                    ts.last_time = now;
                }
            } else if screen == ScreenState::TrackPicker {
                let dy = y - ts.last_y;
                let dt = ts.last_time.elapsed().as_secs_f32();

                let mut tp = state.interaction.track_physics.borrow_mut();
                tp.is_moving = true;
                tp.offset_y += dy;

                if dt > 0.001 {
                    let inst_v = dy / dt;
                    tp.velocity = inst_v * 0.7 + tp.velocity * 0.3;
                    ts.last_time = now;
                }
            }
        }
    }

    ts.last_x = x;
    ts.last_y = y;
}

fn handle_touch_up(state: &AppState, ui_weak: &slint::Weak<AppWindow>, raw_x: f32, raw_y: f32) {
    *state.interaction.last_interaction.borrow_mut() = Instant::now();

    let (drag, duration, fired, start_x, start_y, start_off_x, x, y, _is_alphabet) = {
        let mut ts = state.interaction.touch.borrow_mut();
        if !ts.active {
            return;
        }
        ts.active = false;
        let (x, y) = transform_touch(raw_x, raw_y);
        ts.last_x = x;
        ts.last_y = y;
        (
            ts.is_drag,
            ts.start_time.map(|t| t.elapsed().as_millis()).unwrap_or(0),
            ts.long_press_fired,
            ts.start_x,
            ts.start_y,
            ts.start_offset_x,
            x,
            y,
            ts.is_alphabet,
        )
    };
    {
        let mut ts = state.interaction.touch.borrow_mut();
        ts.is_alphabet = false;
    }

    let dx = x - start_x;
    let dy = y - start_y;

    if let Some(u) = ui_weak.upgrade() {
        let screen = u.get_current_screen();

        if screen == ScreenState::Selector {
            let start_off_x = start_off_x;
            screens::selector::handle_touch_up(state, &u, x, y, dx, dy, drag, fired, start_off_x);
        } else if screen == ScreenState::Player {
            screens::player::handle_touch_up(state, &u, x, y, dx, dy, drag, fired);
        } else if screen == ScreenState::TrackPicker {
            screens::track_picker::handle_touch_up(state, &u, x, y, dx, dy, drag, fired);
        }
    }
}

/// Comprueba si hay una pulsación larga activa y abre el TrackPicker.
/// Devuelve false si no se pudo obtener el borrow de touch (el tick debe abortar).
pub fn check_long_press(
    state: &AppState,
    ui_weak: &slint::Weak<AppWindow>,
    now: Instant,
) -> bool {
    let Ok(mut ts) = state.interaction.touch.try_borrow_mut() else {
        return false;
    };
    if let Some(start) = ts.start_time {
        if ts.active
            && !ts.is_drag
            && now.duration_since(start).as_millis() > LONG_PRESS_MS
            && !ts.long_press_fired
        {
            ts.long_press_fired = true;
            if let Some(ui) = ui_weak.upgrade() {
                // Al hacer pulsación larga, abrimos el selector de canciones del disco centrado
                let s = state.interaction.swiper.borrow();
                let albums = state.library.albums.borrow();
                let target_idx = s.lib_offset + 3; // El centro

                if target_idx >= 0 && (target_idx as usize) < albums.len() {
                    let album = &albums[target_idx as usize];
                    log::info!("Long Press: Cargando canciones para {}", album.title);

                    if let Some(tracks) = &album.tracks {
                        {
                            let mut ids = state.library.track_ids.borrow_mut();
                            ids.clear();
                            for t in tracks {
                                ids.push(t.track_id.clone());
                            }

                            let mut tp = state.interaction.track_physics.borrow_mut();
                            tp.offset_y = 0.0;
                            tp.velocity = 0.0;
                            let total_h = (tracks.len() as f32) * TRACK_ITEM_HEIGHT;
                            let viewport_h = TRACK_LIST_Y_END - TRACK_LIST_Y_START;
                            tp.min_offset = if total_h > viewport_h {
                                viewport_h - total_h
                            } else {
                                0.0
                            };
                        }

                        let slint_tracks: Vec<TrackData> = tracks
                            .iter()
                            .map(|t| {
                                let dur_sec = t.duration.unwrap_or(0.0);
                                let dur_text = format!(
                                    "{}:{:02}",
                                    (dur_sec / 60.0) as i32,
                                    (dur_sec % 60.0) as i32
                                );
                                TrackData {
                                    track_id: t.track_id.clone().into(),
                                    title: t.title.clone().into(),
                                    artist: t
                                        .artist
                                        .clone()
                                        .unwrap_or_else(|| "---".to_string())
                                        .into(),
                                    duration_text: dur_text.into(),
                                    track_number: t.track_number.unwrap_or(0),
                                }
                            })
                            .collect();

                        let model = Rc::new(slint::VecModel::from(slint_tracks));
                        ui.set_current_tracks(model.into());
                        ui.set_album_title(album.title.clone().into());
                        ui.set_album_artist(
                            album
                                .album_artist
                                .clone()
                                .unwrap_or_else(|| "---".to_string())
                                .into(),
                        );
                        ui.set_current_screen(ScreenState::TrackPicker);
                    }
                }
            }
        }
    }
    true
}
