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
    let x_pos: Vec<f32> = (-CENTER_INDEX..=CENTER_INDEX)
        .map(|i: i32| CENTER_X + (i as f32) * state.swiper.borrow().spacing)
        .collect();
    ui.set_x_positions(Rc::new(VecModel::from(x_pos)).into());
    ui.set_center_index(CENTER_INDEX);

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
                let mut dt = now.duration_since(*last).as_secs_f32();
                *last = now;
                // CAP de dt para evitar saltos locos en la física si hay lag
                if dt > 0.05 { dt = 0.05; }
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
                                s.lib_offset = target_idx - CENTER_INDEX;
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
                
                // Trigger windowed pre-loading
                if let (Ok(mut img_s), Ok(mode)) = (
                    state.image_state.try_borrow_mut(),
                    state.current_mode.try_borrow(),
                ) {
                    let s = state.swiper.borrow();
                    ui_utils::preload_neighborhood(
                        &mode,
                        &state.albums.borrow(),
                        &state.playlists.borrow(),
                        &mut img_s,
                        &state.img_tx,
                        s.lib_offset,
                    );
                }

                if let Some(ui) = ui_weak.upgrade() {
                    if let (Ok(mut img_s), Ok(mode)) = (
                        state.image_state.try_borrow_mut(),
                        state.current_mode.try_borrow(),
                    ) {
                        let s = state.swiper.borrow();
                        let albums = state.albums.borrow();
                        let playlists = state.playlists.borrow();
                        for i in 0..VISIBLE_SLOTS {
                            state.model.set_row_data(
                                i as usize,
                                get_item_slint(
                                    &mode,
                                    &albums,
                                    &playlists,
                                    &mut img_s,
                                    &state.img_tx,
                                    s.lib_offset + i,
                                ),
                            );
                        }
                        if let Some(item_data) = state.model.row_data(CENTER_INDEX as usize) {
                            ui.set_bg_cover(item_data.cover.clone());
                        }
                    }
                }
            }

            // 6. Procesamiento de imágenes asíncronas (limitado a 2 por frame para fluidez, como en Python)
            let mut loaded_any = false;
            let mut uploaded_count = 0;
            while let Ok((path, width, height, pixels)) = img_rx.try_recv() {
                log::info!("Image: Received loaded pixels for {}", path);
                let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                    &pixels, width, height,
                );
                let img = slint::Image::from_rgba8(buffer);
                if let Ok(mut img_s) = state.image_state.try_borrow_mut() {
                    img_s.cache.insert(path.clone(), img);
                    img_s.loading.remove(&path);
                }
                loaded_any = true;
                uploaded_count += 1;
                if uploaded_count >= 2 {
                    break;
                }
            }
            
            // Cleanup de caché una vez por tick si se cargaron imágenes
            if loaded_any {
                if let (Ok(mut img_s), Ok(mode)) = (state.image_state.try_borrow_mut(), state.current_mode.try_borrow()) {
                    ui_utils::cleanup_cache(
                        &mut img_s, 
                        state.swiper.borrow().lib_offset,
                        &mode,
                        &state.albums.borrow(),
                        &state.playlists.borrow()
                    );
                }
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

                let old_lib_offset = s.lib_offset;
                while s.offset_x >= s.spacing {
                    s.lib_offset -= 1;
                    s.offset_x -= s.spacing;
                    s.snap_target -= s.spacing;
                }
                while s.offset_x <= -s.spacing {
                    s.lib_offset += 1;
                    s.offset_x += s.spacing;
                    s.snap_target += s.spacing;
                }
                
                let lib_delta = s.lib_offset - old_lib_offset;
                let recycled = lib_delta != 0 || loaded_any || recycled_from_warp;

                (
                    s.is_moving || was_moving,
                    s.offset_x,
                    s.spacing,
                    s.lib_offset,
                    lib_delta,
                    recycled,
                    physics_updated,
                )
            };

            let (is_moving, offset_x, spacing, lib_offset, lib_delta, recycled, physics_updated) =
                update_result;

            // 10. Actualización Sincrónica de la UI
            if physics_updated || is_moving || recycled {
                if let Some(ui) = ui_weak.upgrade() {
                    let off = offset_x;
                    let x_pos: Vec<f32> = (-CENTER_INDEX..=CENTER_INDEX)
                        .map(|i| CENTER_X + (i as f32) * spacing + off)
                        .collect();
                    ui.set_x_positions(Rc::new(VecModel::from(x_pos)).into());

                    let shift = (-off / spacing).round() as i32;
                    let visual_center = (CENTER_INDEX + shift).clamp(0, VISIBLE_SLOTS - 1);

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

                            // OPTIMIZACIÓN: Solo recargamos todo si hubo un warp, un cambio de modo 
                            // o si alguna imagen terminó de cargar (para que se vea en su slot).
                            // Si solo es un desplazamiento suave de 1 slot, movemos los datos existentes.
                            if !loaded_any && !recycled_from_warp && lib_delta == 1 {
                                // Desplazamiento a la derecha (lib_offset aumenta): 
                                // Corremos todos los items una posición a la izquierda y cargamos el nuevo a la derecha.
                                for i in 0..(VISIBLE_SLOTS - 1) {
                                    if let Some(d) = state.model.row_data((i + 1) as usize) {
                                        state.model.set_row_data(i as usize, d);
                                    }
                                }
                                state.model.set_row_data((VISIBLE_SLOTS - 1) as usize, get_item_slint(&mode, &albums, &playlists, &mut img_s, &state.img_tx, lib_offset + VISIBLE_SLOTS - 1));
                            } else if !loaded_any && !recycled_from_warp && lib_delta == -1 {
                                // Desplazamiento a la izquierda (lib_offset disminuye):
                                // Corremos todos los items una posición a la derecha y cargamos el nuevo a la izquierda.
                                for i in (1..VISIBLE_SLOTS).rev() {
                                    if let Some(d) = state.model.row_data((i - 1) as usize) {
                                        state.model.set_row_data(i as usize, d);
                                    }
                                }
                                state.model.set_row_data(0, get_item_slint(&mode, &albums, &playlists, &mut img_s, &state.img_tx, lib_offset));
                            } else {
                                // Caso general: recarga completa (Warp, cambio de modo brusco o imagen cargada)
                                for i in 0..VISIBLE_SLOTS {
                                    state.model.set_row_data(
                                        i as usize,
                                        get_item_slint(
                                            &mode,
                                            &albums,
                                            &playlists,
                                            &mut img_s,
                                            &state.img_tx,
                                            lib_offset + i,
                                        ),
                                    );
                                }
                            }

                            // Pre-load neighbors: solo si el offset cambió o si hubo recarga completa.
                            // Esto asegura que la ventana de pre-carga siempre esté al día.
                            if lib_delta != 0 || recycled_from_warp {
                                ui_utils::preload_neighborhood(
                                    &mode,
                                    &albums,
                                    &playlists,
                                    &mut img_s,
                                    &state.img_tx,
                                    lib_offset,
                                );
                            }
                        }
                    }

                    // Fondo: Solicitamos actualización diferida (Lazy Background como en Python)
                    if center_changed || recycled {
                        if let (Ok(mut bg_idx), Ok(mut bg_time)) = (state.last_bg_target_idx.try_borrow_mut(), state.last_bg_update_time.try_borrow_mut()) {
                            *bg_idx = visual_center as i32;
                            *bg_time = now;
                        }
                    }
                }
            }

            // 11. Ejecución diferida del fondo (150ms de margen para fluidez)
            if let (Ok(mut bg_idx), Ok(bg_time)) = (state.last_bg_target_idx.try_borrow_mut(), state.last_bg_update_time.try_borrow()) {
                if *bg_idx != -1 && now.duration_since(*bg_time).as_millis() > 150 {
                    if let Some(ui) = ui_weak.upgrade() {
                        if let Some(item_data) = state.model.row_data(*bg_idx as usize) {
                            if item_data.cover.size().width > 0 {
                                ui.set_bg_cover(item_data.cover.clone());
                                *bg_idx = -1; // Consumido
                            }
                        }
                    }
                }
            }
        },
    );

    ui.run()
}
