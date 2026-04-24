use crate::app::state::AppState;
use crate::config::*;
use crate::ui_utils::go_to_selector;
use crate::AppWindow;

pub fn handle_touch_up(
    state: &AppState,
    ui: &AppWindow,
    x: f32,
    y: f32,
    _dx: f32,
    _dy: f32,
    drag: bool,
    fired: bool,
) {
    // Botón Cerrar (X)
    if x > TRACK_CLOSE_X_MIN && y < TRACK_CLOSE_Y_MAX {
        log::info!("TrackPicker: Cerrar");
        go_to_selector(ui);
        return;
    }

    if !drag && !fired {
        // Click en canción
        let y_in_list = y - TRACK_LIST_Y_START;
        let scroll_off = state.interaction.track_physics.borrow().offset_y;
        let item_idx = ((y_in_list - scroll_off) / TRACK_ITEM_HEIGHT).floor() as i32;

        if item_idx >= 0 && y > TRACK_LIST_Y_START && y < TRACK_LIST_Y_END {
            let ids = state.library.track_ids.borrow();
            if item_idx < ids.len() as i32 {
                let tid = ids[item_idx as usize].clone();
                log::info!("TrackPicker: Pista {} pulsada (id={})", item_idx, tid);
                ui.invoke_track_clicked(tid.into());
            }
        }
    }
}
