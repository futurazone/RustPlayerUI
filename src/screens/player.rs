//! Controlador de la pantalla Player.
//!
//! Gestiona los eventos touch-up del reproductor usando hitboxes manuales
//! (coordenadas calibradas en config.rs para 1280x720):
//! - Fila de controles (Y≈420): Prev, Play/Pause, Next
//! - Fila de opciones (Y≈585): Shuffle, Repeat
//! - Swipe horizontal a la izquierda (dx < -50px): vuelve al Selector.
use crate::app::state::AppState;
use crate::config::*;
use crate::ui_utils::go_to_selector;
use crate::AppWindow;

pub fn handle_touch_up(
    _state: &AppState,
    ui: &AppWindow,
    x: f32,
    y: f32,
    dx: f32,
    _dy: f32,
    drag: bool,
    fired: bool,
) {
    if !drag && !fired {
        // --- GESTIÓN MANUAL DEL TOUCH EN PLAYER ---
        if (y - PLAYER_CONTROLS_Y).abs() < BUTTON_HIT_RADIUS {
            if (x - PLAYER_PREV_X).abs() < BUTTON_HIT_RADIUS {
                log::info!("Player Touch: PREV");
                ui.invoke_play_prev();
            } else if (x - PLAYER_PLAY_X).abs() < BUTTON_HIT_RADIUS {
                log::info!("Player Touch: PLAY/PAUSE");
                ui.invoke_toggle_pause();
            } else if (x - PLAYER_NEXT_X).abs() < BUTTON_HIT_RADIUS {
                log::info!("Player Touch: NEXT");
                ui.invoke_play_next();
            }
        } else if (y - PLAYER_OPTIONS_Y).abs() < BUTTON_HIT_RADIUS {
            if (x - PLAYER_SHUFFLE_X).abs() < BUTTON_HIT_RADIUS {
                log::info!("Player Touch: SHUFFLE");
                ui.invoke_toggle_shuffle();
            } else if (x - PLAYER_REPEAT_X).abs() < BUTTON_HIT_RADIUS {
                log::info!("Player Touch: REPEAT");
                ui.invoke_toggle_repeat();
            }
        }
    }

    if dx < EXIT_SWIPE_THRESHOLD {
        log::info!("Interaccion: SWIPE LEFT -> Salir Reproductor");
        go_to_selector(ui);
    }
}
