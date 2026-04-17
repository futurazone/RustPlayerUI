mod api;
mod config;
mod physics;
mod touch;
mod ui_utils;

slint::include_modules!();

use slint::{ComponentHandle, Image, Model, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::config::*;
use crate::touch::*;
use crate::ui_utils::*;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum WarpState {
    None,
    Exiting {
        start_time: Instant,
        duration: f32,
        direction: f32,
        target_idx: i32,
    },
    Entering {
        start_time: Instant,
        duration: f32,
        direction: f32,
    },
}

fn find_nearest_album(target_char: char, albums: &[api::Album]) -> i32 {
    if target_char == '#' {
        for (i, alb) in albums.iter().enumerate() {
            if let Some(first) = alb.album_artist.as_deref().and_then(|s| s.chars().next()) {
                if !char::is_alphabetic(first) {
                    return i as i32;
                }
            }
        }
        return 0;
    }

    let target_val = target_char.to_ascii_uppercase() as i32;
    let mut best_idx = -1;
    let mut min_dist = i32::MAX;

    for (i, alb) in albums.iter().enumerate() {
        if let Some(first) = alb.album_artist.as_deref().and_then(|s| s.chars().next()) {
            let first_val = first.to_ascii_uppercase() as i32;
            let dist = (first_val - target_val).abs();
            if dist < min_dist {
                min_dist = dist;
                best_idx = i as i32;
            }
        }
    }

    if best_idx == -1 {
        return (albums.len() as i32) - 1;
    }
    best_idx
}

fn main() -> Result<(), slint::PlatformError> {
    env_logger::init();
    log::info!("Starting Pi Player Rust UI...");
    let ui = AppWindow::new()?;

    let api_url = std::env::var("API_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());

    // Iconos
    ui.set_icon_play(Image::load_from_path("assets/play.svg".as_ref()).unwrap_or_default());
    ui.set_icon_pause(Image::load_from_path("assets/pause.svg".as_ref()).unwrap_or_default());
    ui.set_icon_prev(Image::load_from_path("assets/prev.svg".as_ref()).unwrap_or_default());
    ui.set_icon_next(Image::load_from_path("assets/next.svg".as_ref()).unwrap_or_default());
    ui.set_icon_shuffle(Image::load_from_path("assets/shuffle.svg".as_ref()).unwrap_or_default());
    ui.set_icon_repeat(Image::load_from_path("assets/repeat.svg".as_ref()).unwrap_or_default());
    ui.set_icon_library(Image::load_from_path("assets/library.svg".as_ref()).unwrap_or_default());

    // Estado inicial de la UI
    go_to_selector(&ui);

    let mut swiper_logic = physics::SwiperPhysics::new();
    swiper_logic.spacing = SWIPER_SPACING;
    let current_mode = Rc::new(RefCell::new(api::BrowserMode::Albums));

    let (lib_tx, lib_rx) = mpsc::channel();
    let (status_tx, status_rx) = mpsc::channel();

    // Hilo de carga de biblioteca
    {
        let api_url_clone = api_url.clone();
        std::thread::spawn(move || {
            let albums = api::fetch_real_albums(&api_url_clone).unwrap_or_default();
            let playlists = api::fetch_real_playlists(&api_url_clone).unwrap_or_default();
            let _ = lib_tx.send((albums, playlists));
        });
    }

    // Hilo de status polling (cada 1s)
    {
        let api_url_clone = api_url.clone();
        std::thread::spawn(move || loop {
            if let Ok(status) = api::get_real_status(&api_url_clone) {
                let _ = status_tx.send(status);
            }
            std::thread::sleep(Duration::from_secs(1));
        });
    }

    let initial_albums = Vec::new();
    let initial_playlists = Vec::new();

    let albums_ref = Rc::new(RefCell::new(initial_albums));
    let playlists_ref = Rc::new(RefCell::new(initial_playlists));
    let image_state = Rc::new(RefCell::new(ImageState::default()));
    let (img_tx, img_rx) = mpsc::channel();

    let model = Rc::new(VecModel::default());
    for i in 0..7 {
        model.push(get_item_slint(
            &current_mode.borrow(),
            &albums_ref.borrow(),
            &playlists_ref.borrow(),
            &mut image_state.borrow_mut(),
            &img_tx,
            swiper_logic.lib_offset + i as i32,
        ));
    }
    ui.set_visible_items(model.clone().into());

    if !albums_ref.borrow().is_empty() {
        if let Some(path) = &albums_ref.borrow()[0].cover {
            ui.set_bg_cover(Image::load_from_path(path.as_ref()).unwrap_or_default());
        }
    }

    let center_x_pos = CENTER_X;
    let x_pos: Vec<f32> = (-3..=3)
        .map(|i: i32| center_x_pos + (i as f32) * swiper_logic.spacing)
        .collect();
    ui.set_x_positions(Rc::new(VecModel::from(x_pos)).into());
    ui.set_center_index(3);

    let swiper_state = Rc::new(RefCell::new(swiper_logic));
    let track_physics = Rc::new(RefCell::new(physics::VerticalPhysics::new()));
    let last_interaction = Rc::new(RefCell::new(Instant::now()));
    let last_stop_time = Rc::new(RefCell::new(None));
    let playback_state = Rc::new(RefCell::new(String::from("stop")));
    let track_ids_ref = Rc::new(RefCell::new(Vec::<String>::new()));
    let warp_state = Rc::new(RefCell::new(WarpState::None));
    let albums_ref = Rc::new(RefCell::new(Vec::<api::Album>::new()));

    // --- NUEVO: Estado Optimista y de Interpolación ---
    let opt_shuffle = Rc::new(RefCell::new(false));
    let opt_repeat = Rc::new(RefCell::new(false));
    let opt_lock = Rc::new(RefCell::new(Instant::now() - Duration::from_secs(5)));
    let last_sync_pos = Rc::new(RefCell::new(0.0f32));
    let last_sync_dur = Rc::new(RefCell::new(1.0f32));
    let last_sync_time = Rc::new(RefCell::new(Instant::now()));

    // Estado Global de Touch
    let touch_state = Rc::new(RefCell::new(TouchState::default()));

    {
        let t_state_d = touch_state.clone();
        let s_phys_d = swiper_state.clone();
        let tp_phys_d = track_physics.clone();
        let tids_d = track_ids_ref.clone();
        let warp_d = warp_state.clone();
        let albums_d = albums_ref.clone();
        let ui_h_d = ui.as_weak();

        let li_d = last_interaction.clone();
        ui.on_global_touch_down(move |raw_x, raw_y| {
            *li_d.borrow_mut() = Instant::now();
            let (x, y) = transform_touch(raw_x, raw_y);
            let mut ts = t_state_d.borrow_mut();
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

            let mut s = s_phys_d.borrow_mut();
            ts.start_offset_x = s.offset_x;
            ts.start_offset_y = tp_phys_d.borrow().offset_y;

            if s.is_moving && s.velocity.abs() > 100.0 {
                s.velocity *= 0.3; // "Catch" effect from Python
            } else {
                s.velocity = 0.0;
            }

            let current_screen = ui_h_d
                .upgrade()
                .map(|u| u.get_current_screen())
                .unwrap_or(ScreenState::Selector);

            if y < 90.0 && current_screen == ScreenState::Selector {
                ts.is_alphabet = true;
                let calib_f = ((x - (SCREEN_WIDTH as f32 * 0.08))
                    / (SCREEN_WIDTH as f32 * (0.85 - 0.08)))
                    .clamp(0.0, 1.0);
                let alphabet = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ";
                let char_idx = (calib_f * (alphabet.len() as f32 - 1.0)).round() as usize;
                let target_char = alphabet.chars().nth(char_idx).unwrap_or('#');
                log::info!(
                    "ALPHABET TOUCH (DOWN): '{}' (x={:.1}, f={:.2})",
                    target_char,
                    x,
                    calib_f
                );

                // Trigger instant jump on Down
                if let (Ok(mut warp), Ok(full_albs)) =
                    (warp_d.try_borrow_mut(), albums_d.try_borrow())
                {
                    if *warp == WarpState::None && !full_albs.is_empty() {
                        let target_idx = find_nearest_album(target_char, &full_albs);
                        if target_idx != -1 {
                            let curr_idx = (s.lib_offset + 3).rem_euclid(full_albs.len() as i32);
                            let n = full_albs.len() as i32;
                            let r_steps = (target_idx - curr_idx).rem_euclid(n);
                            let l_steps = (curr_idx - target_idx).rem_euclid(n);
                            let dir = if r_steps <= l_steps { -1.0 } else { 1.0 };

                            *warp = WarpState::Exiting {
                                start_time: Instant::now(),
                                duration: 0.25,
                                direction: dir,
                                target_idx,
                            };
                            log::info!(
                                "Warp Jump (DOWN): Letter {} -> Target Album {} (dir={})",
                                target_char,
                                target_idx,
                                dir
                            );
                        }
                    }
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
        });

        // 2. TOUCH MOVE
        let t_state_m = touch_state.clone();
        let ui_h_m = ui.as_weak();
        let s_phys_m = swiper_state.clone();
        let tp_phys_m = track_physics.clone();
        let tids_m = track_ids_ref.clone();
        let warp_m = warp_state.clone();
        let albums_m = albums_ref.clone();

        let li_m = last_interaction.clone();
        ui.on_global_touch_move(move |raw_x, raw_y| {
            *li_m.borrow_mut() = Instant::now();
            let now = Instant::now();
            let mut ts = t_state_m.borrow_mut();
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
                let calib_f = ((x - (SCREEN_WIDTH as f32 * 0.08))
                    / (SCREEN_WIDTH as f32 * (0.85 - 0.08)))
                    .clamp(0.0, 1.0);
                let alphabet = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ";
                let idx = (calib_f * (alphabet.len() as f32 - 1.0)).round() as usize;
                let target_char = alphabet.chars().nth(idx).unwrap_or('#');

                // --- Lógica de Salto (Warp) ---
                if let (Ok(mut warp), Ok(full_albs)) =
                    (warp_m.try_borrow_mut(), albums_m.try_borrow())
                {
                    if *warp == WarpState::None && !full_albs.is_empty() {
                        let target_idx = find_nearest_album(target_char, &full_albs);

                        if target_idx != -1 {
                            let curr_idx = (s_phys_m.borrow().lib_offset + 3)
                                .rem_euclid(full_albs.len() as i32);
                            let n = full_albs.len() as i32;
                            let r_steps = (target_idx - curr_idx).rem_euclid(n);
                            let l_steps = (curr_idx - target_idx).rem_euclid(n);
                            let dir = if r_steps <= l_steps { -1.0 } else { 1.0 };

                            *warp = WarpState::Exiting {
                                start_time: Instant::now(),
                                duration: 0.25,
                                direction: dir,
                                target_idx,
                            };
                            log::info!(
                                "Warp Jump (MOVE): Letter {} -> Target Album {} (dir={})",
                                target_char,
                                target_idx,
                                dir
                            );
                        }
                    }
                }
            } else if ts.is_drag {
                if let Some(u) = ui_h_m.upgrade() {
                    let screen = u.get_current_screen();
                    if screen == ScreenState::Selector {
                        let dx = x - ts.last_x;
                        let dt = ts.last_time.elapsed().as_secs_f32();

                        let mut s = s_phys_m.borrow_mut();
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

                        let mut tp = tp_phys_m.borrow_mut();
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
        });

        // 3. TOUCH UP
        let t_state_u = touch_state.clone();
        let s_phys_u = swiper_state.clone();
        let tp_phys_u = track_physics.clone();
        let tids_u = track_ids_ref.clone();
        let warp_u = warp_state.clone();
        let albums_u = albums_ref.clone();
        let m_ref_u = current_mode.clone();
        let api_url_tap = api_url.clone();
        let ui_handle2 = ui.as_weak();
        let model = model.clone();

        let li_u = last_interaction.clone();
        ui.on_global_touch_up(move |raw_x, raw_y| {
            *li_u.borrow_mut() = Instant::now();

            let (drag, duration, fired, start_x, start_y, start_off_x, x, y, is_alphabet) = {
                let mut ts = t_state_u.borrow_mut();
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
                let mut ts = t_state_u.borrow_mut();
                ts.is_alphabet = false;
            }

            let dx = x - start_x;
            let dy = y - start_y;

            if let Some(u) = ui_handle2.upgrade() {
                let screen = u.get_current_screen();

                if screen == ScreenState::Selector {
                    let (s_offset_x, s_spacing) = {
                        let Ok(s) = s_phys_u.try_borrow() else {
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
                                if *m_ref_u.borrow() == api::BrowserMode::Albums {
                                    if let Some(item_data) = model.row_data(3) {
                                        log::info!("Navigation: Go to Player (TAP Center)");
                                        u.set_album_title(item_data.title.clone());
                                        u.set_album_artist(item_data.artist.clone());
                                        u.set_bg_cover(item_data.cover.clone());
                                        u.set_current_screen(ScreenState::Player);

                                        let albums = albums_u.borrow();
                                        if let Some(album_data) = albums.get(3) {
                                            if let Some(tracks) = &album_data.tracks {
                                                let track_ids: Vec<String> = tracks
                                                    .iter()
                                                    .map(|t| t.track_id.clone())
                                                    .collect();
                                                let api = api_url_tap.clone();
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
                                if let Ok(mut mut_s) = s_phys_u.try_borrow_mut() {
                                    let target_snap = slot as f32 * s_spacing;
                                    mut_s.snap_target = mut_s.offset_x + target_snap;
                                    mut_s.is_moving = true;
                                    mut_s.velocity = 0.0;
                                }
                            }
                        }
                    } else if drag {
                        if let Ok(mut s) = s_phys_u.try_borrow_mut() {
                            let vel = s.velocity;
                            let off = s.offset_x;
                            s.set_snap_slot(off, vel);
                        }
                    }
                } else if screen == ScreenState::Player {
                    if !drag && !fired {
                        // --- GESTIÓN MANUAL DEL TOUCH EN PLAYER ---
                        // Usamos lógica analítica similar al swiper para detectar pulsaciones
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
                        let scroll_off = tp_phys_u.borrow().offset_y;
                        let item_idx =
                            ((y_in_list - scroll_off) / TRACK_ITEM_HEIGHT).floor() as i32;

                        if item_idx >= 0 && y > TRACK_LIST_Y_START && y < TRACK_LIST_Y_END {
                            let ids = tids_u.borrow();
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
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_back_to_selector(move || {
            log::info!("Gesture: back to selector");
            if let Some(ui) = ui_handle.upgrade() {
                go_to_selector(&ui);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_close_track_picker(move || {
            log::info!("Modal: close track picker");
            if let Some(ui) = ui_handle.upgrade() {
                if ui.get_current_screen() == ScreenState::TrackPicker {
                    go_to_selector(&ui);
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let api_url_clone = api_url.clone();
        ui.on_track_clicked(move |track_id| {
            log::info!("Track clicked: id={}", track_id);
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_current_screen(ScreenState::Player);
                let api = api_url_clone.clone();
                let tid = track_id.to_string();
                std::thread::spawn(move || {
                    let _ = api::play_track(&api, &tid);
                });
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let mode_toggle = current_mode.clone();
        let a_ref = albums_ref.clone();
        let p_ref = playlists_ref.clone();
        let mod_target = model.clone();
        let sw_state = swiper_state.clone();
        let i_state = image_state.clone();
        let tx = img_tx.clone();

        ui.on_toggle_browser_mode(move || {
            let new_mode = {
                let mut mode = mode_toggle.borrow_mut();
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

            if let Some(ui) = ui_handle.upgrade() {
                ui.set_browser_mode(if new_mode == api::BrowserMode::Albums {
                    BrowserMode::Albums
                } else {
                    BrowserMode::Playlists
                });

                let lib_off = {
                    let mut s = sw_state.borrow_mut();
                    s.lib_offset = -3;
                    s.offset_x = 0.0;
                    s.snap_target = 0.0;
                    s.velocity = 0.0;
                    s.lib_offset
                };

                let mut img_s = i_state.borrow_mut();
                let albums = a_ref.borrow();
                let playlists = p_ref.borrow();
                for i in 0..7 {
                    mod_target.set_row_data(
                        i,
                        get_item_slint(
                            &new_mode,
                            &albums,
                            &playlists,
                            &mut img_s,
                            &tx,
                            lib_off + i as i32,
                        ),
                    );
                }

                if let Some(item_data) = mod_target.row_data(3) {
                    ui.set_bg_cover(item_data.cover.clone());
                }
            }
        });
    }

    // ALBUM CLICKED implementation
    {
        let ui_handle = ui.as_weak();
        let albums_click_model = model.clone();

        ui.on_album_clicked(move |visual_idx| {
            if let Some(ui) = ui_handle.upgrade() {
                if let Some(item_data) = albums_click_model.row_data(visual_idx as usize) {
                    log::info!("Navigation: Go to Player (Click visual_idx={})", visual_idx);
                    ui.set_album_title(item_data.title.clone());
                    ui.set_album_artist(item_data.artist.clone());
                    ui.set_bg_cover(item_data.cover.clone());
                    ui.set_current_screen(ScreenState::Player);
                }
            }
        });
    }

    // PLAYER ACTIONS
    {
        let api_url_clone = api_url.clone();
        ui.on_toggle_pause(move || {
            let api = api_url_clone.clone();
            std::thread::spawn(move || {
                let _ = api::send_player_command(&api, "pause");
            });
        });
    }
    {
        let api_url_clone = api_url.clone();
        ui.on_play_next(move || {
            let api = api_url_clone.clone();
            std::thread::spawn(move || {
                let _ = api::send_player_command(&api, "seek/fwd");
            });
        });
    }
    {
        let api_url_clone = api_url.clone();
        ui.on_play_prev(move || {
            let api = api_url_clone.clone();
            std::thread::spawn(move || {
                let _ = api::send_player_command(&api, "seek/back");
            });
        });
    }
    {
        let api_url_clone = api_url.clone();
        let ui_handle = ui.as_weak();
        let opt_shuffle_cb = opt_shuffle.clone();
        let opt_lock_cb = opt_lock.clone();
        ui.on_toggle_shuffle(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let mut opt = opt_shuffle_cb.borrow_mut();
                *opt = !*opt;
                ui.set_shuffle_on(*opt);
                *opt_lock_cb.borrow_mut() = Instant::now();

                let api = api_url_clone.clone();
                std::thread::spawn(move || {
                    let _ = api::send_player_command(&api, "shuffle");
                });
            }
        });
    }
    {
        let api_url_clone = api_url.clone();
        let ui_handle = ui.as_weak();
        let opt_repeat_cb = opt_repeat.clone();
        let opt_lock_cb = opt_lock.clone();
        ui.on_toggle_repeat(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let mut opt = opt_repeat_cb.borrow_mut();
                *opt = !*opt;
                ui.set_repeat_on(*opt);
                *opt_lock_cb.borrow_mut() = Instant::now();

                let api = api_url_clone.clone();
                std::thread::spawn(move || {
                    let _ = api::send_player_command(&api, "repeat");
                });
            }
        });
    }

    let last_tick = Rc::new(RefCell::new(Instant::now()));
    let s_tick = swiper_state.clone();
    let albums_tick = albums_ref.clone();
    let playlists_tick = playlists_ref.clone();
    let mode_tick = current_mode.clone();
    let model_tick = model.clone();
    let ui_tick = ui.as_weak();
    let i_state_tick = image_state.clone();
    let tx_tick = img_tx.clone();
    let tids_tick = track_ids_ref.clone();
    let tp_tick = track_physics.clone();
    let warp_tick = warp_state.clone();
    let albums_tick = albums_ref.clone();
    let opt_shuffle_tick = opt_shuffle.clone();
    let opt_repeat_tick = opt_repeat.clone();
    let opt_lock_tick = opt_lock.clone();
    let last_sync_pos_tick = last_sync_pos.clone();
    let last_sync_dur_tick = last_sync_dur.clone();
    let last_sync_time_tick = last_sync_time.clone();

    let t_state_tick = touch_state.clone();
    let li_tick = last_interaction.clone();
    let lst_tick = last_stop_time.clone();
    let ps_tick = playback_state.clone();

    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(16),
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
            {
                let Ok(mut ts) = t_state_tick.try_borrow_mut() else {
                    return;
                };
                if let Some(start) = ts.start_time {
                    if ts.active
                        && !ts.is_drag
                        && now.duration_since(start).as_millis() > LONG_PRESS_MS
                        && !ts.long_press_fired
                    {
                        ts.long_press_fired = true;
                        if let Some(ui) = ui_tick.upgrade() {
                            // Al hacer pulsación larga, abrimos el selector de canciones del disco centrado
                            let s = s_tick.borrow();
                            let albums = albums_tick.borrow();
                            let target_idx = s.lib_offset + 3; // El centro

                            if target_idx >= 0 && (target_idx as usize) < albums.len() {
                                let album = &albums[target_idx as usize];
                                log::info!("Long Press: Cargando canciones para {}", album.title);

                                if let Some(tracks) = &album.tracks {
                                    {
                                        let mut ids = tids_tick.borrow_mut();
                                        ids.clear();
                                        for t in tracks {
                                            ids.push(t.track_id.clone());
                                        }

                                        let mut tp = tp_tick.borrow_mut();
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
            }

            // 2. Procesamiento de Status del Player (Polling cada 1s)
            if let Ok(status) = status_rx.try_recv() {
                if let Some(ui) = ui_tick.upgrade() {
                    // Update Metadata
                    ui.set_current_track_title(
                        status.title.clone().unwrap_or_else(|| "---".into()).into(),
                    );
                    ui.set_current_track_artist(
                        status.artist.clone().unwrap_or_else(|| "---".into()).into(),
                    );
                    ui.set_current_track_album(
                        status.album.clone().unwrap_or_else(|| "---".into()).into(),
                    );

                    // Update Buttons (Optimistic Check)
                    let is_playing_now = status.state.as_deref() == Some("play");
                    ui.set_is_playing(is_playing_now);

                    if opt_lock_tick.borrow().elapsed().as_secs_f32() > 2.0 {
                        let sh = status.shuffle.unwrap_or(false);
                        let rep = status.repeat.unwrap_or(false);
                        *opt_shuffle_tick.borrow_mut() = sh;
                        *opt_repeat_tick.borrow_mut() = rep;
                        ui.set_shuffle_on(sh);
                        ui.set_repeat_on(rep);
                    }

                    // Update Progress Sync
                    let pos = status.position.unwrap_or(0.0);
                    let dur = status.duration.unwrap_or(1.0);
                    *last_sync_pos_tick.borrow_mut() = pos;
                    *last_sync_dur_tick.borrow_mut() = dur;
                    *last_sync_time_tick.borrow_mut() = Instant::now();

                    if !is_playing_now {
                        ui.set_progress_pos(if dur > 0.0 {
                            (pos / dur).clamp(0.0, 1.0)
                        } else {
                            0.0
                        });
                        let mins = (pos as i32) / 60;
                        let secs = (pos as i32) % 60;
                        ui.set_time_label(format!("{}:{:02}", mins, secs).into());
                    }

                    // Watchdog: Automático a Player si empieza la música
                    let old_state = ps_tick.borrow().clone();
                    let new_status_state = status.state.clone().unwrap_or_else(|| "stop".into());

                    if new_status_state == "play" && old_state != "play" {
                        if ui.get_current_screen() == ScreenState::Selector {
                            log::info!("Watchdog: Música detectada, saltando a Player");
                            ui.set_current_screen(ScreenState::Player);
                        }
                    }

                    if new_status_state != "play" && old_state == "play" {
                        *lst_tick.borrow_mut() = Some(Instant::now());
                    } else if new_status_state == "play" {
                        *lst_tick.borrow_mut() = None;
                    }

                    *ps_tick.borrow_mut() = new_status_state;
                }
            }

            // --- NUEVO: Gestión de Warp State Machine ---
            let mut recycled_from_warp = false;
            {
                if let Ok(mut ws) = warp_tick.try_borrow_mut() {
                    match *ws {
                        WarpState::Exiting {
                            start_time,
                            duration,
                            direction,
                            target_idx,
                        } => {
                            let elapsed = start_time.elapsed().as_secs_f32();
                            let progress = (elapsed / duration).clamp(0.0, 1.0);

                            if let Some(ui) = ui_tick.upgrade() {
                                ui.set_warp_opacity(1.0 - progress);
                                ui.set_warp_offset(
                                    (direction * progress * (SCREEN_WIDTH * 0.7)).into(),
                                );
                            }

                            if elapsed >= duration {
                                if let Ok(mut s) = s_tick.try_borrow_mut() {
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
                        }
                        WarpState::Entering {
                            start_time,
                            duration,
                            direction,
                        } => {
                            let elapsed = start_time.elapsed().as_secs_f32();
                            let progress = (elapsed / duration).clamp(0.0, 1.0);

                            if let Some(ui) = ui_tick.upgrade() {
                                ui.set_warp_opacity(progress);
                                ui.set_warp_offset(
                                    (-direction * (1.0 - progress) * (SCREEN_WIDTH * 0.7)).into(),
                                );
                            }

                            if elapsed >= duration {
                                if let Some(ui) = ui_tick.upgrade() {
                                    ui.set_warp_opacity(1.0);
                                    ui.set_warp_offset(0.0.into());
                                }
                                *ws = WarpState::None;
                            }
                        }
                        WarpState::None => {}
                    }
                }
            }

            // 3. Watchdog de Inactividad (Retorno a Selector)
            let now_ins = Instant::now();
            let inactive_duration = now_ins.duration_since(*li_tick.borrow());

            if let Some(ui) = ui_tick.upgrade() {
                if ui.get_current_screen() == ScreenState::Player {
                    let state = ps_tick.borrow().clone();
                    if state != "play" {
                        // Si no hay hora de parada grabada (ej. entramos con música ya parada), usamos la última interacción
                        let stop_reference = lst_tick.borrow().unwrap_or(*li_tick.borrow());

                        if now_ins.duration_since(stop_reference).as_secs() > 30
                            && inactive_duration.as_secs() > 30
                        {
                            log::info!(
                                "Watchdog: 30s de inactividad en Player, volviendo al selector"
                            );
                            ui.set_current_screen(ScreenState::Selector);
                            *lst_tick.borrow_mut() = None;
                        }
                    }
                }
            }

            // 4. Procesamiento de datos de la API (Biblioteca)
            if let Ok((new_albums, new_playlists)) = lib_rx.try_recv() {
                log::info!("API: Real data received, updating UI models...");
                *albums_ref.borrow_mut() = new_albums;
                *playlists_ref.borrow_mut() = new_playlists;
                if let Some(ui) = ui_tick.upgrade() {
                    if let (Ok(mut img_s), Ok(mode)) =
                        (i_state_tick.try_borrow_mut(), mode_tick.try_borrow())
                    {
                        let s = s_tick.borrow();
                        let albums = albums_tick.borrow();
                        let playlists = playlists_tick.borrow();
                        for i in 0..7 {
                            model_tick.set_row_data(
                                i,
                                get_item_slint(
                                    &mode,
                                    &albums,
                                    &playlists,
                                    &mut img_s,
                                    &tx_tick,
                                    s.lib_offset + i as i32,
                                ),
                            );
                        }
                        if let Some(item_data) = model_tick.row_data(3) {
                            ui.set_bg_cover(item_data.cover.clone());
                        }
                    }
                }
            }

            // 3. Procesamiento de imágenes asíncronas
            let mut loaded_any = false;
            while let Ok((path, width, height, pixels)) = img_rx.try_recv() {
                let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                    &pixels, width, height,
                );
                let img = slint::Image::from_rgba8(buffer);
                if let Ok(mut img_s) = i_state_tick.try_borrow_mut() {
                    img_s.cache.insert(path.clone(), img);
                    img_s.loading.remove(&path);
                }
                loaded_any = true;
            }

            // --- INTERPOLACIÓN LOCAL DEL TIEMPO (PROGRESS BAR) ---
            if *ps_tick.borrow() == "play" {
                let elapsed = last_sync_time_tick.borrow().elapsed().as_secs_f32();
                let current_pos = *last_sync_pos_tick.borrow() + elapsed;
                let dur = *last_sync_dur_tick.borrow();
                if let Some(ui) = ui_tick.upgrade() {
                    ui.set_progress_pos(if dur > 0.0 {
                        (current_pos / dur).clamp(0.0, 1.0)
                    } else {
                        0.0
                    });
                    let mins = (current_pos as i32) / 60;
                    let secs = (current_pos as i32) % 60;
                    ui.set_time_label(slint::format!("{}:{:02}", mins, secs));
                }
            }

            // --- NUEVO: Física Vertical (TrackPicker) ---
            let tp_updated = {
                if let Ok(mut tp) = tp_tick.try_borrow_mut() {
                    tp.update(dt)
                } else {
                    false
                }
            };

            if tp_updated {
                if let Some(ui) = ui_tick.upgrade() {
                    ui.set_track_list_offset(tp_tick.borrow().offset_y.into());
                }
            }

            // 4. Update de Física y Reciclaje
            let update_result = {
                let Ok(mut s) = s_tick.try_borrow_mut() else {
                    return;
                };
                let Ok(ts) = t_state_tick.try_borrow() else {
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

            // 5. Actualización Sincrónica de la UI (Fuera de borrows principales)
            if physics_updated || is_moving || recycled {
                if let Some(ui) = ui_tick.upgrade() {
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
                        if let (Ok(mut img_s), Ok(mode)) =
                            (i_state_tick.try_borrow_mut(), mode_tick.try_borrow())
                        {
                            let albums = albums_tick.borrow();
                            let playlists = playlists_tick.borrow();
                            for i in 0..7 {
                                model_tick.set_row_data(
                                    i,
                                    get_item_slint(
                                        &mode,
                                        &albums,
                                        &playlists,
                                        &mut img_s,
                                        &tx_tick,
                                        lib_offset + i as i32,
                                    ),
                                );
                            }
                        }
                    }

                    // Fondo actualizado SIEMPRE al final para usar datos frescos
                    if center_changed || recycled {
                        if let Some(item_data) = model_tick.row_data(visual_center as usize) {
                            // Solo actualizamos si hay una imagen real cargada (no default transparente)
                            // Esto evita que el fondo "se pierda" durante la carga
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
