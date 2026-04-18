//! Estado del touch y calibración de coordenadas.
//!
//! `TouchState` trackea el gesto activo (inicio, posición, drag, long press).
//! `transform_touch` convierte coordenadas raw del touchscreen del Pi (rotado 90°)
//! a coordenadas de pantalla 1280×720. En macOS pasa las coordenadas tal cual.

use std::time::Instant;

pub struct TouchState {
    pub active: bool,
    pub start_time: Option<Instant>,
    pub last_time: Instant,
    pub start_x: f32,
    pub start_y: f32,
    pub last_x: f32,
    pub last_y: f32,
    pub is_drag: bool,
    pub long_press_fired: bool,
    pub start_offset_x: f32,
    pub is_alphabet: bool,
    pub start_offset_y: f32,
}

impl Default for TouchState {
    fn default() -> Self {
        Self {
            active: false,
            start_time: None,
            last_time: Instant::now(),
            start_x: 0.0,
            start_y: 0.0,
            last_x: 0.0,
            last_y: 0.0,
            is_drag: false,
            long_press_fired: false,
            start_offset_x: 0.0,
            is_alphabet: false,
            start_offset_y: 0.0,
        }
    }
}

pub fn transform_touch(x: f32, y: f32) -> (f32, f32) {
    if cfg!(target_arch = "arm") {
        // Pi Touch Calibration (Ajustado para 1280x720)
        let tx = (652.0 - y) / 588.0 * 1280.0;
        let ty = (1120.0 - x) / 1016.0 * 720.0;
        (tx, ty)
    } else {
        (x, y)
    }
}
