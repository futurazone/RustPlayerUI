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

mod app;
mod api;
mod callbacks;
mod config;
mod physics;
mod player_sync;
mod screens;
mod services;
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

use crate::app::Application;
use crate::app::state::AppState;
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
    
    // Cargar splash desde la ruta del backend
    ui.set_splash_image(Image::load_from_path("../backend/splash_design.png".as_ref()).unwrap_or_default());
}



fn main() -> Result<(), slint::PlatformError> {
    env_logger::init();
    log::info!("Starting Pi Player Rust UI...");

    // Inicialización de la aplicación y estado centralizado
    let (app, img_rx) = Application::init()?;
    let ui = app.ui;
    let state = app.state;

    load_icons(&ui);

    // Hilos de background
    let services::ServiceChannels { lib_rx, status_rx } = services::spawn_all_services(&state.api_url);

    // Registrar handlers
    touch_handlers::register_touch_handlers(&ui, &state);
    callbacks::register_callbacks(&ui, &state);

    // Timer tick principal (~60fps)
    let last_tick = Rc::new(RefCell::new(Instant::now()));
    let ui_weak = ui.as_weak();
    let state = state.clone();

    let app_start_time = Instant::now();
    let timer = slint::Timer::default();
    let mut splash_done = false;
    let mut backend_ready_time: Option<Instant> = None;

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

            // 0. Control del SplashScreen (Startup Security Delay)
            if !splash_done {
                let has_data = !state.library.albums.borrow().is_empty();
                if has_data {
                    if backend_ready_time.is_none() {
                        backend_ready_time = Some(now);
                    }
                    if let Some(ready_t) = backend_ready_time {
                        let backend_safe = now.duration_since(ready_t).as_secs_f32() > 0.5;
                        let min_time_passed = now.duration_since(app_start_time).as_secs_f32() > 1.0;

                        if backend_safe && min_time_passed {
                            splash_done = true;
                            if let Some(ui) = ui_weak.upgrade() {
                                let is_playing = *state.playback.playback_state.borrow() == "play";
                                if is_playing {
                                    log::info!("STARTUP: Música detectada, saltando a Player");
                                    ui.set_current_screen(crate::ScreenState::Player);
                                } else {
                                    log::info!("STARTUP: Listo, saltando a Selector");
                                    ui.set_current_screen(crate::ScreenState::Selector);
                                }
                            }
                        }
                    }
                }
            }

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
            if let Ok(mut ws) = state.interaction.warp.try_borrow_mut() {
                if let Some(ui) = ui_weak.upgrade() {
                    match warp::process_warp_tick(&mut ws, &ui) {
                        warp::WarpTickResult::ExitingComplete {
                            target_idx,
                            direction,
                        } => {
                            if let Ok(mut s) = state.interaction.swiper.try_borrow_mut() {
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
                
                *state.library.albums.borrow_mut() = new_albums;
                *state.library.playlists.borrow_mut() = new_playlists;
                
                // Trigger windowed pre-loading
                if let (Ok(mut img_s), Ok(mode)) = (
                    state.library.image_state.try_borrow_mut(),
                    state.library.current_mode.try_borrow(),
                ) {
                    let s = state.interaction.swiper.borrow();
                    ui_utils::preload_neighborhood(
                        &mode,
                        &state.library.albums.borrow(),
                        &state.library.playlists.borrow(),
                        &mut img_s,
                        &state.library.img_tx,
                        s.lib_offset,
                    );
                }

                if let Some(ui) = ui_weak.upgrade() {
                    if let (Ok(mut img_s), Ok(mode)) = (
                        state.library.image_state.try_borrow_mut(),
                        state.library.current_mode.try_borrow(),
                    ) {
                        let s = state.interaction.swiper.borrow();
                        let albums = state.library.albums.borrow();
                        let playlists = state.library.playlists.borrow();
                        for i in 0..VISIBLE_SLOTS {
                            state.library.model.set_row_data(
                                i as usize,
                                get_item_slint(
                                    &mode,
                                    &albums,
                                    &playlists,
                                    &mut img_s,
                                    &state.library.img_tx,
                                    s.lib_offset + i,
                                ),
                            );
                        }
                        if let Some(item_data) = state.library.model.row_data(CENTER_INDEX as usize) {
                            ui.set_bg_cover(item_data.cover.clone());
                        }
                    }
                }
            }

            // 6. Procesamiento de imágenes asíncronas (limitado a 2 por frame para fluidez, como en Python)
            let mut loaded_any = false;
            let mut uploaded_count = 0;
            let mut player_cover_update: Option<Image> = None;

            while let Ok((path, width, height, pixels)) = img_rx.try_recv() {
                log::info!("Image: Received loaded pixels for {}", path);
                let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                    &pixels, width, height,
                );
                let img = slint::Image::from_rgba8(buffer);
                
                // Check if this is the player cover before moving img
                let current_track = state.playback.last_track_id.borrow();
                if let Some(track_id) = current_track.as_ref() {
                    let albums = state.library.albums.borrow();
                    for album in albums.iter() {
                        if let Some(tracks) = &album.tracks {
                            for track in tracks {
                                if &track.track_id == track_id {
                                    if let Some(cp) = album.cover_thumb.as_ref().or(album.cover.as_ref()) {
                                        if cp == &path {
                                            player_cover_update = Some(img.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Ok(mut img_s) = state.library.image_state.try_borrow_mut() {
                    img_s.cache.insert(path.clone(), img);
                    img_s.loading.remove(&path);
                }
                loaded_any = true;
                uploaded_count += 1;
                if uploaded_count >= 2 {
                    break;
                }
            }

            // Apply player cover update after all borrows are done
            if let Some(player_cover) = player_cover_update {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_player_cover(player_cover);
                }
            }
            
            // Cleanup de caché una vez por tick si se cargaron imágenes
            if loaded_any {
                if let (Ok(mut img_s), Ok(mode)) = (state.library.image_state.try_borrow_mut(), state.library.current_mode.try_borrow()) {
                    ui_utils::cleanup_cache(
                        &mut img_s, 
                        state.interaction.swiper.borrow().lib_offset,
                        &mode,
                        &state.library.albums.borrow(),
                        &state.library.playlists.borrow()
                    );
                }
            }

            // 7. Interpolación local del tiempo (Progress Bar)
            if let Some(ui) = ui_weak.upgrade() {
                player_sync::interpolate_progress(&state, &ui);
            }

            // 8. Física Vertical (TrackPicker)
            let tp_updated = if let Ok(mut tp) = state.interaction.track_physics.try_borrow_mut() {
                tp.update(dt)
            } else {
                false
            };

            if tp_updated {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_track_list_offset(state.interaction.track_physics.borrow().offset_y.into());
                }
            }

            // DEFERRED BG UPDATE: Ejecutar aquí para que funcione aunque el swiper esté parado
            // (el early return de abajo salta el Paso 11 si no hay movimiento)
            if let (Ok(mut bg_idx), Ok(bg_time)) = (state.library.last_bg_target_idx.try_borrow_mut(), state.library.last_bg_update_time.try_borrow()) {
                if *bg_idx != -1 && now.duration_since(*bg_time).as_millis() > 150 {
                    if let Some(ui) = ui_weak.upgrade() {
                        if let Some(item_data) = state.library.model.row_data(*bg_idx as usize) {
                            if item_data.cover.size().width > 0 {
                                ui.set_bg_cover(item_data.cover.clone());
                                *bg_idx = -1; // Consumido
                            }
                        }
                    }
                }
            }

            // 9. Update de Física y Reciclaje del Swiper
            let update_result = {
                let Ok(mut s) = state.interaction.swiper.try_borrow_mut() else {
                    return;
                };
                let Ok(ts) = state.interaction.touch.try_borrow() else {
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
                            state.library.image_state.try_borrow_mut(),
                            state.library.current_mode.try_borrow(),
                        ) {
                            let albums = state.library.albums.borrow();
                            let playlists = state.library.playlists.borrow();

                            // OPTIMIZACIÓN: Solo recargamos todo si hubo un warp, un cambio de modo 
                            // o si alguna imagen terminó de cargar (para que se vea en su slot).
                            // Si solo es un desplazamiento suave de 1 slot, movemos los datos existentes.
                            if !loaded_any && !recycled_from_warp && lib_delta == 1 {
                                // Desplazamiento a la derecha (lib_offset aumenta): 
                                // Corremos todos los items una posición a la izquierda y cargamos el nuevo a la derecha.
                                for i in 0..(VISIBLE_SLOTS - 1) {
                                    if let Some(d) = state.library.model.row_data((i + 1) as usize) {
                                        state.library.model.set_row_data(i as usize, d);
                                    }
                                }
                                state.library.model.set_row_data((VISIBLE_SLOTS - 1) as usize, get_item_slint(&mode, &albums, &playlists, &mut img_s, &state.library.img_tx, lib_offset + VISIBLE_SLOTS - 1));
                            } else if !loaded_any && !recycled_from_warp && lib_delta == -1 {
                                // Desplazamiento a la izquierda (lib_offset disminuye):
                                // Corremos todos los items una posición a la derecha y cargamos el nuevo a la izquierda.
                                for i in (1..VISIBLE_SLOTS).rev() {
                                    if let Some(d) = state.library.model.row_data((i - 1) as usize) {
                                        state.library.model.set_row_data(i as usize, d);
                                    }
                                }
                                state.library.model.set_row_data(0, get_item_slint(&mode, &albums, &playlists, &mut img_s, &state.library.img_tx, lib_offset));
                            } else {
                                // Caso general: recarga completa (Warp, cambio de modo brusco o imagen cargada)
                                for i in 0..VISIBLE_SLOTS {
                                    state.library.model.set_row_data(
                                        i as usize,
                                        get_item_slint(
                                            &mode,
                                            &albums,
                                            &playlists,
                                            &mut img_s,
                                            &state.library.img_tx,
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
                                    &state.library.img_tx,
                                    lib_offset,
                                );
                            }
                        }
                    }

                    // Fondo: Solicitamos actualización diferida (Lazy Background como en Python)
                    if center_changed || recycled {
                        if let (Ok(mut bg_idx), Ok(mut bg_time)) = (state.library.last_bg_target_idx.try_borrow_mut(), state.library.last_bg_update_time.try_borrow_mut()) {
                            *bg_idx = visual_center as i32;
                            *bg_time = now;
                        }
                    }
                }
            }

            // 11. (El fondo diferido ahora se ejecuta antes del early return del Paso 9)
        },
    );

    ui.run()
}
