//! Sincronización del estado del reproductor con el servidor MPD.
//!
//! - `process_status_update`: procesa cada poll de status (cada 1s) → actualiza
//!   metadata, botones (con check optimista), progress sync, y watchdog auto-play
//! - `interpolate_progress`: entre polls, interpola la posición local para que
//!   la barra de progreso avance suavemente (~60fps)
//! - `check_inactivity_watchdog`: vuelve al Selector si el Player lleva 30s
//!   parado sin interacción del usuario

use std::time::Instant;

use crate::api;
use crate::app_state::AppState;
use crate::ScreenState;

/// Procesa una actualización de estado del reproductor desde el hilo de polling.
pub fn process_status_update(status: &api::PlayerStatus, state: &AppState, ui: &crate::AppWindow) {
    // Update Metadata
    ui.set_current_track_title(
        status
            .title
            .clone()
            .unwrap_or_else(|| "---".into())
            .into(),
    );
    ui.set_current_track_artist(
        status
            .artist
            .as_ref()
            .or(status.artist_name.as_ref())
            .or(status.album_artist.as_ref())
            .cloned()
            .unwrap_or_else(|| "---".into())
            .into(),
    );
    ui.set_current_track_album(
        status
            .album
            .clone()
            .unwrap_or_else(|| "---".into())
            .into(),
    );

    // Update Buttons (Optimistic Check)
    let is_playing_now = status.state.as_deref() == Some("play");
    ui.set_is_playing(is_playing_now);

    if state.opt_lock.borrow().elapsed().as_secs_f32() > 2.0 {
        let sh = status.shuffle.unwrap_or(false);
        let rep = status.repeat.unwrap_or(false);
        *state.opt_shuffle.borrow_mut() = sh;
        *state.opt_repeat.borrow_mut() = rep;
        ui.set_shuffle_on(sh);
        ui.set_repeat_on(rep);
    }

    // Update Progress Sync
    let pos = status.position.unwrap_or(0.0);
    let dur = status.duration.unwrap_or(0.0).max(1.0); // Evitar división por cero
    
    // Si ha cambiado la canción, reseteamos progreso visual inmediatamente
    let new_track_id = status.track_id.clone();
    let mut last_id = state.last_track_id.borrow_mut();
    if new_track_id != *last_id {
        log::info!("Sync: New track detected, resetting progress bar");
        ui.set_progress_pos(0.0);
        ui.set_time_label("0:00".into());
        *last_id = new_track_id;
    }

    *state.last_sync_pos.borrow_mut() = pos;
    *state.last_sync_dur.borrow_mut() = dur;
    *state.last_sync_time.borrow_mut() = Instant::now();

    // Actualización inmediata si no estamos interpolando (o si acabamos de recibir el status)
    ui.set_progress_pos((pos / dur).clamp(0.0, 1.0));
    let mins = (pos as i32) / 60;
    let secs = (pos as i32) % 60;
    ui.set_time_label(slint::format!("{}:{:02}", mins, secs));

    // Watchdog: Automático a Player si empieza la música
    let old_state = state.playback_state.borrow().clone();
    let new_status_state = status.state.clone().unwrap_or_else(|| "stop".into());

    if new_status_state == "play" && old_state != "play" {
        if ui.get_current_screen() == ScreenState::Selector {
            log::info!("Watchdog: Música detectada, saltando a Player");
            ui.set_current_screen(ScreenState::Player);
        }
    }

    if new_status_state != "play" && old_state == "play" {
        *state.last_stop_time.borrow_mut() = Some(Instant::now());
    } else if new_status_state == "play" {
        *state.last_stop_time.borrow_mut() = None;
    }

    *state.playback_state.borrow_mut() = new_status_state;
}

/// Comprueba la inactividad y vuelve al selector si corresponde (30s timeout).
pub fn check_inactivity_watchdog(state: &AppState, ui: &crate::AppWindow) {
    let now = Instant::now();
    let inactive_duration = now.duration_since(*state.last_interaction.borrow());

    if ui.get_current_screen() == ScreenState::Player {
        let ps = state.playback_state.borrow().clone();
        if ps != "play" {
            // Si no hay hora de parada grabada, usamos la última interacción
            let stop_reference = state
                .last_stop_time
                .borrow()
                .unwrap_or(*state.last_interaction.borrow());

            if now.duration_since(stop_reference).as_secs() > 30
                && inactive_duration.as_secs() > 30
            {
                log::info!(
                    "Watchdog: 30s de inactividad en Player, volviendo al selector"
                );
                ui.set_current_screen(ScreenState::Selector);
                *state.last_stop_time.borrow_mut() = None;
            }
        }
    }
}

/// Interpola la posición de reproducción entre updates del servidor.
pub fn interpolate_progress(state: &AppState, ui: &crate::AppWindow) {
    if *state.playback_state.borrow() == "play" {
        let elapsed = state.last_sync_time.borrow().elapsed().as_secs_f32();
        let current_pos = *state.last_sync_pos.borrow() + elapsed;
        let dur = *state.last_sync_dur.borrow();
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
