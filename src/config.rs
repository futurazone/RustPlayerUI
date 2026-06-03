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

pub const DRAG_THRESHOLD_SQ: f32 = 1600.0; // 40px radius (era 22px — demasiado sensible)
pub const LONG_PRESS_MS: u128 = 400;
pub const TAP_MAX_DURATION_MS: u128 = 250; // 250ms como Python (era 2000!)
pub const TAP_OFFSET_THRESHOLD: f32 = 16.0; // 16px como Python

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

// Shutdown button position and size (bottom-right corner)
pub const SHUTDOWN_BTN_X: f32 = SCREEN_WIDTH - 125.0;
pub const SHUTDOWN_BTN_Y: f32 = SCREEN_HEIGHT - 125.0;
pub const SHUTDOWN_BTN_SIZE: f32 = 100.0;
// Zona amplia de disparo: 250px desde la derecha, 200px desde abajo
pub const SHUTDOWN_HIT_X: f32 = SCREEN_WIDTH - 250.0;
pub const SHUTDOWN_HIT_Y: f32 = SCREEN_HEIGHT - 200.0;
pub const SHUTDOWN_AUTO_HIDE_SECS: u64 = 8;

// Preload sliding window (Cyclic)
pub const PRELOAD_WINDOW_HALF: i32 = 50;        // 50 a cada lado → ~101 covers en caché
pub const WINDOW_SHIFT_THRESHOLD: i32 = 25;      // 25 pasos de swipe → shift window

// Swiper slots configuration
pub const VISIBLE_SLOTS: i32 = 9;
pub const CENTER_INDEX: i32 = VISIBLE_SLOTS / 2;
