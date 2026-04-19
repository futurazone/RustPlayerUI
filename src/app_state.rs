//! Estado centralizado de la aplicación.
//!
//! `AppState` agrupa todos los `Rc<RefCell<>>` en un solo struct clonable.
//! Cada closure de Slint recibe `state.clone()` (clona punteros Rc, no datos).
//! Esto reemplaza los ~30 clones manuales que había en la versión monolítica.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use slint::VecModel;

use crate::api;
use crate::config::SWIPER_SPACING;
use crate::physics;
use crate::touch::TouchState;
use crate::ui_utils::{get_item_slint, ImageState};
use crate::warp::WarpState;
use crate::AlbumData;

/// Estado centralizado de la aplicación.
/// Todos los campos son `Rc<RefCell<>>` para compartir entre closures de Slint.
/// Implementa `Clone` (clona los Rc, no los datos).
#[derive(Clone)]
pub struct AppState {
    pub swiper: Rc<RefCell<physics::SwiperPhysics>>,
    pub track_physics: Rc<RefCell<physics::VerticalPhysics>>,
    pub touch: Rc<RefCell<TouchState>>,
    pub warp: Rc<RefCell<WarpState>>,
    pub albums: Rc<RefCell<Vec<api::Album>>>,
    pub playlists: Rc<RefCell<Vec<api::Playlist>>>,
    pub current_mode: Rc<RefCell<api::BrowserMode>>,
    pub image_state: Rc<RefCell<ImageState>>,
    pub track_ids: Rc<RefCell<Vec<String>>>,
    pub model: Rc<VecModel<AlbumData>>,
    pub img_tx: mpsc::Sender<(String, u32, u32, Vec<u8>)>,
    pub last_interaction: Rc<RefCell<Instant>>,
    pub playback_state: Rc<RefCell<String>>,
    pub last_stop_time: Rc<RefCell<Option<Instant>>>,
    // Estado optimista de toggles
    pub opt_shuffle: Rc<RefCell<bool>>,
    pub opt_repeat: Rc<RefCell<bool>>,
    pub opt_lock: Rc<RefCell<Instant>>,
    // Interpolación de progreso
    pub last_sync_pos: Rc<RefCell<f32>>,
    pub last_sync_dur: Rc<RefCell<f32>>,
    pub last_sync_time: Rc<RefCell<Instant>>,
    // Persistencia de posición entre modos (offset_x, lib_offset)
    pub albums_pos: Rc<RefCell<(f32, i32)>>,
    pub playlists_pos: Rc<RefCell<(f32, i32)>>,
    pub last_track_id: Rc<RefCell<Option<String>>>,
    // API
    pub api_url: String,
}

impl AppState {
    /// Crea el estado inicial de la aplicación.
    /// Devuelve (AppState, img_rx) donde img_rx recibe imágenes cargadas async.
    pub fn new(api_url: String) -> (Self, mpsc::Receiver<(String, u32, u32, Vec<u8>)>) {
        let (img_tx, img_rx) = mpsc::channel();

        let mut swiper = physics::SwiperPhysics::new();
        swiper.spacing = SWIPER_SPACING;

        let albums: Rc<RefCell<Vec<api::Album>>> = Rc::new(RefCell::new(Vec::new()));
        let playlists: Rc<RefCell<Vec<api::Playlist>>> = Rc::new(RefCell::new(Vec::new()));
        let image_state = Rc::new(RefCell::new(ImageState::default()));
        let current_mode = Rc::new(RefCell::new(api::BrowserMode::Albums));

        // Modelo visual inicial (7 slots vacíos, se rellenan cuando llegan datos de la API)
        let model = Rc::new(VecModel::default());
        for i in 0..7 {
            model.push(get_item_slint(
                &current_mode.borrow(),
                &albums.borrow(),
                &playlists.borrow(),
                &mut image_state.borrow_mut(),
                &img_tx,
                swiper.lib_offset + i as i32,
            ));
        }

        let state = Self {
            swiper: Rc::new(RefCell::new(swiper)),
            track_physics: Rc::new(RefCell::new(physics::VerticalPhysics::new())),
            touch: Rc::new(RefCell::new(TouchState::default())),
            warp: Rc::new(RefCell::new(WarpState::None)),
            albums,
            playlists,
            current_mode,
            image_state,
            track_ids: Rc::new(RefCell::new(Vec::new())),
            model,
            img_tx,
            last_interaction: Rc::new(RefCell::new(Instant::now())),
            playback_state: Rc::new(RefCell::new(String::from("stop"))),
            last_stop_time: Rc::new(RefCell::new(None)),
            opt_shuffle: Rc::new(RefCell::new(false)),
            opt_repeat: Rc::new(RefCell::new(false)),
            opt_lock: Rc::new(RefCell::new(Instant::now() - Duration::from_secs(5))),
            last_sync_pos: Rc::new(RefCell::new(0.0f32)),
            last_sync_dur: Rc::new(RefCell::new(1.0f32)),
            last_sync_time: Rc::new(RefCell::new(Instant::now())),
            albums_pos: Rc::new(RefCell::new((0.0f32, -3))),
            playlists_pos: Rc::new(RefCell::new((0.0f32, -3))),
            last_track_id: Rc::new(RefCell::new(None)),
            api_url,
        };

        (state, img_rx)
    }
}
