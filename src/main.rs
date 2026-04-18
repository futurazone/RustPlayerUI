//! Punto de entrada de PiPlayer Rust UI.
//!
//! Orquesta la inicialización y conecta los módulos:
//! - `app_state` → estado compartido (AppState con Rc<RefCell<>>)
//! - `touch_handlers` → eventos touch globales (down/move/up + long press)
//! - `callbacks` → callbacks de UI de Slint (botones, navegación)
//! - `player_sync` → sincronización con el servidor MPD (status, progress, watchdog)
//! - `warp` → animación de salto rápido por letra del alfabeto
//! - `physics` → física del carrusel horizontal y scroll vertical
//!
//! El timer tick (~60fps) coordina: long press → status → warp → watchdog →
//! biblioteca → imágenes async → progress → física vertical → swiper + reciclaje.

mod api;
mod app_state;
mod callbacks;
mod config;
mod physics;
mod player_sync;
mod touch;
mod touch_handlers;
mod ui_utils;
mod warp;

slint::include_modules!();

use slint::{ComponentHandle, Image, Model, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::app_state::AppState;
use crate::config::*;
use crate::ui_utils::*;
use crate::warp::WarpState;

fn load_icons(ui: &AppWindow) {
    ui.set_icon_play(Image::load_from_path("assets/play.svg".as_ref()).unwrap_or_default());
    ui.set_icon_pause(Image::load_from_path("assets/pause.svg".as_ref()).unwrap_or_default());
    ui.set_icon_prev(Image::load_from_path("assets/prev.svg".as_ref()).unwrap_or_default());
    ui.set_icon_next(Image::load_from_path("assets/next.svg".as_ref()).unwrap_or_default());
    ui.set_icon_shuffle(Image::load_from_path("assets/shuffle.svg".as_ref()).unwrap_or_default());
    ui.set_icon_repeat(Image::load_from_path("assets/repeat.svg".as_ref()).unwrap_or_default());
    ui.set_icon_library(Image::load_from_path("assets/library.svg".as_ref()).unwrap_or_default());
}

fn spawn_background_threads(
    api_url: &str,
) -> (
    mpsc::Receiver<(Vec<api::Album>, Vec<api::Playlist>)>,
    mpsc::Receiver<api::PlayerStatus>,
) {
    let (lib_tx, lib_rx) = mpsc::channel();
    let (status_tx, status_rx) = mpsc::channel();

    // Hilo de carga de biblioteca
    {
        let api_url = api_url.to_string();
        std::thread::spawn(move || {
            let albums = api::fetch_real_albums(&api_url).unwrap_or_default();
            let playlists = api::fetch_real_playlists(&api_url).unwrap_or_default();
            let _ = lib_tx.send((albums, playlists));
        });
    }

    // Hilo de status polling (cada 1s)
    {
        let api_url = api_url.to_string();
        std::thread::spawn(move || loop {
            if let Ok(status) = api::get_real_status(&api_url) {
                let _ = status_tx.send(status);
            }
            std::thread::sleep(Duration::from_secs(1));
        });
    }

    (lib_rx, status_rx)
}

fn main() -> Result<(), slint::PlatformError> {
    env_logger::init();
    log::info!("Starting Pi Player Rust UI...");
    let ui = AppWindow::new()?;

    let api_url = std::env::var("API_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());

    // Iconos y estado inicial
    load_icons(&ui);
    go_to_selector(&ui);

    // Estado centralizado
    let (state, img_rx) = AppState::new(api_url.clone());

    // Inicializar UI con modelo
    ui.set_visible_items(state.model.clone().into());
    let x_pos: Vec<f32> = (-3..=3)
        .map(|i: i32| CENTER_X + (i as f32) * state.swiper.borrow().spacing)
        .collect();
    ui.set_x_positions(Rc::new(VecModel::from(x_pos)).into());
    ui.set_center_index(3);

    // Hilos de background
    let (lib_rx, status_rx) = spawn_background_threads(&api_url);

    // Registrar handlers
    touch_handlers::register_touch_handlers(&ui, &state);
    callbacks::register_callbacks(&ui, &state);

    // Timer tick principal (~60fps)
    let last_tick = Rc::new(RefCell::new(Instant::now()));
    let ui_weak = ui.as_weak();
    let state = state.clone();

    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            let now = Instant::now();
            let dt = {
                let Ok(mut last) = last_tick.try_borrow_mut() else {
                    return;
                };
                let dt = now.duration_since(*last).as_secs_f32();
                *last = now;
                dt
            };

            // 1. Comprobación de Long-Press (TouchState)
            if !touch_handlers::check_long_press(&state, &ui_weak, now) {
                return;
            }

            // 2. Procesamiento de Status del Player (Polling cada 1s)
            if let Ok(status) = status_rx.try_recv() {
                if let Some(ui) = ui_weak.upgrade() {
                    player_sync::process_status_update(&status, &state, &ui);
                }
            }

            // 3. Gestión de Warp State Machine
            let mut recycled_from_warp = false;
            if let Ok(mut ws) = state.warp.try_borrow_mut() {
                if let Some(ui) = ui_weak.upgrade() {
                    match warp::process_warp_tick(&mut ws, &ui) {
                        warp::WarpTickResult::ExitingComplete {
                            target_idx,
                            direction,
                        } => {
                            if let Ok(mut s) = state.swiper.try_borrow_mut() {
                                s.lib_offset = target_idx - 3;
                                s.offset_x = 0.0;
                                s.snap_target = 0.0;
                                s.velocity = 0.0;
                                recycled_from_warp = true;

                                *ws = WarpState::Entering {
                                    start_time: Instant::now(),
                                    duration: 0.35,
                                    direction,
                                };
                            }
                        }
                        _ => {}
                    }
                }
            }

            // 4. Watchdog de Inactividad (Retorno a Selector)
            if let Some(ui) = ui_weak.upgrade() {
                player_sync::check_inactivity_watchdog(&state, &ui);
            }

            // 5. Procesamiento de datos de la API (Biblioteca)
            if let Ok((new_albums, new_playlists)) = lib_rx.try_recv() {
                log::info!("API: Real data received, updating UI models...");
                *state.albums.borrow_mut() = new_albums;
                *state.playlists.borrow_mut() = new_playlists;
                if let Some(ui) = ui_weak.upgrade() {
                    if let (Ok(mut img_s), Ok(mode)) = (
                        state.image_state.try_borrow_mut(),
                        state.current_mode.try_borrow(),
                    ) {
                        let s = state.swiper.borrow();
                        let albums = state.albums.borrow();
                        let playlists = state.playlists.borrow();
                        for i in 0..7 {
                            state.model.set_row_data(
                                i,
                                get_item_slint(
                                    &mode,
                                    &albums,
                                    &playlists,
                                    &mut img_s,
                                    &state.img_tx,
                                    s.lib_offset + i as i32,
                                ),
                            );
                        }
                        if let Some(item_data) = state.model.row_data(3) {
                            ui.set_bg_cover(item_data.cover.clone());
                        }
                    }
                }
            }

            // 6. Procesamiento de imágenes asíncronas
            let mut loaded_any = false;
            while let Ok((path, width, height, pixels)) = img_rx.try_recv() {
                let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                    &pixels, width, height,
                );
                let img = slint::Image::from_rgba8(buffer);
                if let Ok(mut img_s) = state.image_state.try_borrow_mut() {
                    img_s.cache.insert(path.clone(), img);
                    img_s.loading.remove(&path);
                }
                loaded_any = true;
            }

            // 7. Interpolación local del tiempo (Progress Bar)
            if let Some(ui) = ui_weak.upgrade() {
                player_sync::interpolate_progress(&state, &ui);
            }

            // 8. Física Vertical (TrackPicker)
            let tp_updated = if let Ok(mut tp) = state.track_physics.try_borrow_mut() {
                tp.update(dt)
            } else {
                false
            };

            if tp_updated {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_track_list_offset(state.track_physics.borrow().offset_y.into());
                }
            }

            // 9. Update de Física y Reciclaje del Swiper
            let update_result = {
                let Ok(mut s) = state.swiper.try_borrow_mut() else {
                    return;
                };
                let Ok(ts) = state.touch.try_borrow() else {
                    return;
                };

                if !s.is_moving && !ts.active && !loaded_any && !recycled_from_warp {
                    return;
                }

                let was_moving = s.is_moving;
                let physics_updated = s.update(dt);

                let mut recycled = loaded_any || recycled_from_warp;
                while s.offset_x >= s.spacing {
                    s.lib_offset -= 1;
                    s.offset_x -= s.spacing;
                    s.snap_target -= s.spacing;
                    recycled = true;
                }
                while s.offset_x <= -s.spacing {
                    s.lib_offset += 1;
                    s.offset_x += s.spacing;
                    s.snap_target += s.spacing;
                    recycled = true;
                }
                (
                    s.is_moving || was_moving,
                    s.offset_x,
                    s.spacing,
                    s.lib_offset,
                    recycled,
                    physics_updated,
                )
            };

            let (is_moving, offset_x, spacing, lib_offset, recycled, physics_updated) =
                update_result;

            // 10. Actualización Sincrónica de la UI
            if physics_updated || is_moving || recycled {
                if let Some(ui) = ui_weak.upgrade() {
                    let off = offset_x;
                    let x_pos: Vec<f32> = (-3..=3)
                        .map(|i| CENTER_X + (i as f32) * spacing + off)
                        .collect();
                    ui.set_x_positions(Rc::new(VecModel::from(x_pos)).into());

                    let shift = (-off / spacing).round() as i32;
                    let visual_center = (3 + shift).clamp(0, 6);

                    let center_changed = ui.get_center_index() != visual_center;
                    if center_changed || recycled {
                        ui.set_center_index(visual_center);
                    }

                    if recycled {
                        if let (Ok(mut img_s), Ok(mode)) = (
                            state.image_state.try_borrow_mut(),
                            state.current_mode.try_borrow(),
                        ) {
                            let albums = state.albums.borrow();
                            let playlists = state.playlists.borrow();
                            for i in 0..7 {
                                state.model.set_row_data(
                                    i,
                                    get_item_slint(
                                        &mode,
                                        &albums,
                                        &playlists,
                                        &mut img_s,
                                        &state.img_tx,
                                        lib_offset + i as i32,
                                    ),
                                );
                            }
                        }
                    }

                    // Fondo actualizado SIEMPRE al final para usar datos frescos
                    if center_changed || recycled {
                        if let Some(item_data) = state.model.row_data(visual_center as usize) {
                            // Solo actualizamos si hay una imagen real cargada (no default transparente)
                            if item_data.cover.size().width > 0 {
                                ui.set_bg_cover(item_data.cover.clone());
                            }
                        }
                    }
                }
            }
        },
    );

    ui.run()
}
