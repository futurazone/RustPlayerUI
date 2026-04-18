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
use crate::app_state::AppState;
use crate::config::*;
use crate::touch::transform_touch;
use crate::ui_utils::go_to_selector;
use crate::warp;
use crate::{AppWindow, ScreenState, TrackData};

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
    let calib_f = ((x - (SCREEN_WIDTH as f32 * 0.08))
        / (SCREEN_WIDTH as f32 * (0.85 - 0.08)))
        .clamp(0.0, 1.0);
    let alphabet = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let char_idx = (calib_f * (alphabet.len() as f32 - 1.0)).round() as usize;
    alphabet.chars().nth(char_idx).unwrap_or('#')
}

fn handle_touch_down(state: &AppState, ui_weak: &slint::Weak<AppWindow>, raw_x: f32, raw_y: f32) {
    *state.last_interaction.borrow_mut() = Instant::now();
    let (x, y) = transform_touch(raw_x, raw_y);
    let mut ts = state.touch.borrow_mut();
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

    let mut s = state.swiper.borrow_mut();
    ts.start_offset_x = s.offset_x;
    ts.start_offset_y = state.track_physics.borrow().offset_y;

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
        let calib_f = ((x - (SCREEN_WIDTH as f32 * 0.08))
            / (SCREEN_WIDTH as f32 * (0.85 - 0.08)))
            .clamp(0.0, 1.0);
        log::info!(
            "ALPHABET TOUCH (DOWN): '{}' (x={:.1}, f={:.2})",
            target_char,
            x,
            calib_f
        );

        // Trigger instant jump on Down
        if let (Ok(mut w), Ok(albs)) = (state.warp.try_borrow_mut(), state.albums.try_borrow()) {
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
    *state.last_interaction.borrow_mut() = Instant::now();
    let now = Instant::now();
    let mut ts = state.touch.borrow_mut();
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
        if let (Ok(mut w), Ok(albs)) = (state.warp.try_borrow_mut(), state.albums.try_borrow()) {
            let lib_offset = state.swiper.borrow().lib_offset;
            warp::trigger_warp_jump(&mut w, &albs, lib_offset, target_char, "MOVE");
        }
    } else if ts.is_drag {
        if let Some(u) = ui_weak.upgrade() {
            let screen = u.get_current_screen();
            if screen == ScreenState::Selector {
                let dx = x - ts.last_x;
                let dt = ts.last_time.elapsed().as_secs_f32();

                let mut s = state.swiper.borrow_mut();
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

                let mut tp = state.track_physics.borrow_mut();
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
    *state.last_interaction.borrow_mut() = Instant::now();

    let (drag, duration, fired, start_x, start_y, start_off_x, x, y, _is_alphabet) = {
        let mut ts = state.touch.borrow_mut();
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
        let mut ts = state.touch.borrow_mut();
        ts.is_alphabet = false;
    }

    let dx = x - start_x;
    let dy = y - start_y;

    if let Some(u) = ui_weak.upgrade() {
        let screen = u.get_current_screen();

        if screen == ScreenState::Selector {
            let (s_offset_x, s_spacing) = {
                let Ok(s) = state.swiper.try_borrow() else {
                    return;
                };
                (s.offset_x, s.spacing)
            };
            let offset_diff = (s_offset_x - start_off_x).abs();

            if !drag && !fired && offset_diff < TAP_OFFSET_THRESHOLD {
                // TAP!
                let cx = CENTER_X;
                let slot = ((x - (cx + s_offset_x)) / s_spacing).round() as i32;

                if y >= ALBUM_TAP_Y_MIN
                    && y <= ALBUM_TAP_Y_MAX
                    && duration < TAP_MAX_DURATION_MS
                {
                    if slot == 0 {
                        if *state.current_mode.borrow() == api::BrowserMode::Albums {
                            if let Some(item_data) = state.model.row_data(3) {
                                log::info!("Navigation: Go to Player (TAP Center)");
                                u.set_album_title(item_data.title.clone());
                                u.set_album_artist(item_data.artist.clone());
                                u.set_bg_cover(item_data.cover.clone());
                                u.set_current_screen(ScreenState::Player);

                                let albums = state.albums.borrow();
                                if let Some(album_data) = albums.get(3) {
                                    if let Some(tracks) = &album_data.tracks {
                                        let track_ids: Vec<String> =
                                            tracks.iter().map(|t| t.track_id.clone()).collect();
                                        let api = state.api_url.clone();
                                        std::thread::spawn(move || {
                                            let _ = api::send_queue(&api, track_ids);
                                        });
                                    }
                                }
                            }
                        } else {
                            u.set_current_screen(ScreenState::Player);
                        }
                    } else {
                        log::info!(
                            "Interaccion: TAP Portada Lateral ({}) -> Snapping",
                            slot
                        );
                        if let Ok(mut mut_s) = state.swiper.try_borrow_mut() {
                            let target_snap = slot as f32 * s_spacing;
                            mut_s.snap_target = mut_s.offset_x + target_snap;
                            mut_s.is_moving = true;
                            mut_s.velocity = 0.0;
                        }
                    }
                }
            } else if drag {
                if let Ok(mut s) = state.swiper.try_borrow_mut() {
                    let vel = s.velocity;
                    let off = s.offset_x;
                    s.set_snap_slot(off, vel);
                }
            }
        } else if screen == ScreenState::Player {
            if !drag && !fired {
                // --- GESTIÓN MANUAL DEL TOUCH EN PLAYER ---
                if (y - PLAYER_CONTROLS_Y).abs() < BUTTON_HIT_RADIUS {
                    if (x - PLAYER_PREV_X).abs() < BUTTON_HIT_RADIUS {
                        log::info!("Player Touch: PREV");
                        u.invoke_play_prev();
                    } else if (x - PLAYER_PLAY_X).abs() < BUTTON_HIT_RADIUS {
                        log::info!("Player Touch: PLAY/PAUSE");
                        u.invoke_toggle_pause();
                    } else if (x - PLAYER_NEXT_X).abs() < BUTTON_HIT_RADIUS {
                        log::info!("Player Touch: NEXT");
                        u.invoke_play_next();
                    }
                } else if (y - PLAYER_OPTIONS_Y).abs() < BUTTON_HIT_RADIUS {
                    if (x - PLAYER_SHUFFLE_X).abs() < BUTTON_HIT_RADIUS {
                        log::info!("Player Touch: SHUFFLE");
                        u.invoke_toggle_shuffle();
                    } else if (x - PLAYER_REPEAT_X).abs() < BUTTON_HIT_RADIUS {
                        log::info!("Player Touch: REPEAT");
                        u.invoke_toggle_repeat();
                    }
                }
            }

            if dx < EXIT_SWIPE_THRESHOLD {
                log::info!("Interaccion: SWIPE LEFT -> Salir Reproductor");
                go_to_selector(&u);
            }
        } else if screen == ScreenState::TrackPicker {
            // Botón Cerrar (X)
            if x > TRACK_CLOSE_X_MIN && y < TRACK_CLOSE_Y_MAX {
                log::info!("TrackPicker: Cerrar");
                go_to_selector(&u);
                return;
            }

            if !drag && !fired {
                // Click en canción
                let y_in_list = y - TRACK_LIST_Y_START;
                let scroll_off = state.track_physics.borrow().offset_y;
                let item_idx = ((y_in_list - scroll_off) / TRACK_ITEM_HEIGHT).floor() as i32;

                if item_idx >= 0 && y > TRACK_LIST_Y_START && y < TRACK_LIST_Y_END {
                    let ids = state.track_ids.borrow();
                    if item_idx < ids.len() as i32 {
                        let tid = ids[item_idx as usize].clone();
                        log::info!("TrackPicker: Pista {} pulsada (id={})", item_idx, tid);
                        u.invoke_track_clicked(tid.into());
                    }
                }
            }
        }

        if screen == ScreenState::Selector
            && dy > MODE_SWIPE_DY_MIN
            && dx.abs() < MODE_SWIPE_DX_MAX
        {
            log::info!("Interaccion: SWIPE DOWN -> Toggle Browser Mode");
            u.invoke_toggle_browser_mode();
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
    let Ok(mut ts) = state.touch.try_borrow_mut() else {
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
                let s = state.swiper.borrow();
                let albums = state.albums.borrow();
                let target_idx = s.lib_offset + 3; // El centro

                if target_idx >= 0 && (target_idx as usize) < albums.len() {
                    let album = &albums[target_idx as usize];
                    log::info!("Long Press: Cargando canciones para {}", album.title);

                    if let Some(tracks) = &album.tracks {
                        {
                            let mut ids = state.track_ids.borrow_mut();
                            ids.clear();
                            for t in tracks {
                                ids.push(t.track_id.clone());
                            }

                            let mut tp = state.track_physics.borrow_mut();
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
