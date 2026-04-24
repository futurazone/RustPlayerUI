//! Constantes de layout y thresholds de interacción para 1280×720.
//!
//! Todas las coordenadas están calibradas para la pantalla del Pi.
//! Las zonas de hit-test del Player y TrackPicker se definen aquí.
pub const SCREEN_WIDTH: f32 = 1280.0;
pub const SCREEN_HEIGHT: f32 = 720.0;
pub const CENTER_X: f32 = 640.0;
pub const SWIPER_SPACING: f32 = 550.0;

pub const ALBUM_TAP_Y_MIN: f32 = 170.0;
pub const ALBUM_TAP_Y_MAX: f32 = 510.0;

pub const DRAG_THRESHOLD_SQ: f32 = 1600.0; // 40px radius
pub const LONG_PRESS_MS: u128 = 400;
pub const TAP_MAX_DURATION_MS: u128 = 2000;
pub const TAP_OFFSET_THRESHOLD: f32 = 20.0;

pub const CORNER_TOUCH_SIZE: f32 = 60.0;
pub const EXIT_SWIPE_THRESHOLD: f32 = -50.0;
pub const MODE_SWIPE_DY_MIN: f32 = 70.0;
pub const MODE_SWIPE_DX_MAX: f32 = 50.0;

// Player Screen Hit-Test Zones (1280x720)
// Row 1: Controls (Prev, Play, Next) - Ajustado a columna derecha
pub const PLAYER_CONTROLS_Y: f32 = 420.0; // Ajustado (+50px por padding-top)
pub const PLAYER_PREV_X: f32 = 740.0;
pub const PLAYER_PLAY_X: f32 = 930.0;
pub const PLAYER_NEXT_X: f32 = 1120.0;

// Row 2: Options (Shuffle, Repeat) - Ajustado a columna izquierda (debajo de portada)
pub const PLAYER_OPTIONS_Y: f32 = 585.0; // Ajustado (+50px por padding-top)
pub const PLAYER_SHUFFLE_X: f32 = 135.0; 
pub const PLAYER_REPEAT_X: f32 = 345.0;  // Ajustado según logs (~340-350)

pub const BUTTON_HIT_RADIUS: f32 = 70.0; // Slightly larger for better usability

// Track Picker Constants
pub const TRACK_LIST_Y_START: f32 = 130.0;
pub const TRACK_LIST_Y_END: f32 = 680.0;
pub const TRACK_ITEM_HEIGHT: f32 = 90.0;
pub const TRACK_CLOSE_X_MIN: f32 = 1100.0;
pub const TRACK_CLOSE_Y_MAX: f32 = 150.0;

// Preload range (Cyclic)
pub const PRELOAD_BACKWARD: i32 = 10;
pub const PRELOAD_FORWARD: i32 = 10;

// Swiper slots configuration
pub const VISIBLE_SLOTS: i32 = 7;
pub const CENTER_INDEX: i32 = VISIBLE_SLOTS / 2;
