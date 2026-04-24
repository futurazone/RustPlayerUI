//! Controladores de pantalla del PiPlayer.
//!
//! Cada módulo gestiona la lógica de interacción táctil propia de su pantalla,
//! delegada desde `touch_handlers.rs` según el `ScreenState` activo.
//!
//! - `selector`  → Carrusel de álbumes/playlists, alphabet bar, swipe de modo.
//! - `player`    → Botones de reproducción (Prev/Play/Next/Shuffle/Repeat), swipe de salida.
//! - `track_picker` → Lista de canciones de un álbum/playlist con scroll vertical.
pub mod selector;
pub mod player;
pub mod track_picker;
