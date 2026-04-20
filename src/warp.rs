//! Animación de salto rápido por letra del Alphabet Bar.
//!
//! Máquina de estados: None → Exiting (fade out + slide) → Entering (fade in + slide) → None.
//! `trigger_warp_jump` calcula la dirección más corta (circular) hacia el álbum objetivo.
//! `process_warp_tick` anima cada frame; devuelve `ExitingComplete` cuando el caller
//! debe actualizar el lib_offset del swiper y transicionar a Entering.

use std::time::Instant;

use crate::api;
use crate::config::SCREEN_WIDTH;

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

pub fn find_nearest_album(target_char: char, albums: &[api::Album]) -> i32 {
    let target_char = target_char.to_ascii_uppercase();
    if target_char == '#' {
        for (i, alb) in albums.iter().enumerate() {
            if let Some(first) = alb.album_artist.as_deref().and_then(|s| s.chars().next()) {
                let first_upper = first.to_ascii_uppercase();
                if first_upper < 'A' {
                    return i as i32;
                }
            }
        }
        return 0;
    }

    for (i, alb) in albums.iter().enumerate() {
        if let Some(first) = alb.album_artist.as_deref().and_then(|s| s.chars().next()) {
            if first.to_ascii_uppercase() >= target_char {
                return i as i32;
            }
        }
    }

    (albums.len() as i32).saturating_sub(1)
}

/// Intenta iniciar una animación de warp jump hacia un álbum objetivo.
pub fn trigger_warp_jump(
    warp: &mut WarpState,
    albums: &[api::Album],
    current_lib_offset: i32,
    target_char: char,
    source: &str,
) -> bool {
    if *warp != WarpState::None || albums.is_empty() {
        return false;
    }

    let target_idx = find_nearest_album(target_char, albums);
    if target_idx == -1 {
        return false;
    }

    let curr_idx = (current_lib_offset + 3).rem_euclid(albums.len() as i32);
    let n = albums.len() as i32;
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
        "Warp Jump ({}): Letter {} -> Target Album {} (dir={})",
        source,
        target_char,
        target_idx,
        dir
    );
    true
}

/// Resultado de un tick de warp.
pub enum WarpTickResult {
    /// Sin cambio o animación en progreso.
    NoChange,
    /// La animación de salida completó: el swiper debe saltar a target_idx.
    ExitingComplete { target_idx: i32, direction: f32 },
}

/// Procesa un tick de la máquina de estados de warp.
/// La transición Entering→None se maneja internamente.
/// La transición Exiting→Entering se señaliza con ExitingComplete
/// para que el caller pueda actualizar el swiper antes de transicionar.
pub fn process_warp_tick(ws: &mut WarpState, ui: &crate::AppWindow) -> WarpTickResult {
    match *ws {
        WarpState::Exiting {
            start_time,
            duration,
            direction,
            target_idx,
        } => {
            let elapsed = start_time.elapsed().as_secs_f32();
            let t = (elapsed / duration).clamp(0.0, 1.0);
            let progress = t * t; // Ease-In Quad

            ui.set_warp_opacity(1.0 - progress);
            ui.set_warp_offset((direction * progress * (SCREEN_WIDTH * 0.7)).into());

            if elapsed >= duration {
                WarpTickResult::ExitingComplete {
                    target_idx,
                    direction,
                }
            } else {
                WarpTickResult::NoChange
            }
        }
        WarpState::Entering {
            start_time,
            duration,
            direction,
        } => {
            let elapsed = start_time.elapsed().as_secs_f32();
            let t = (elapsed / duration).clamp(0.0, 1.0);
            let progress = t * (2.0 - t); // Ease-Out Quad

            ui.set_warp_opacity(progress);
            ui.set_warp_offset((-direction * (1.0 - progress) * (SCREEN_WIDTH * 0.7)).into());

            if elapsed >= duration {
                ui.set_warp_opacity(1.0);
                ui.set_warp_offset(0.0.into());
                *ws = WarpState::None;
            }
            WarpTickResult::NoChange
        }
        WarpState::None => WarpTickResult::NoChange,
    }
}
