use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use slint::VecModel;

use crate::api;
use crate::config::{CENTER_INDEX, SWIPER_SPACING, VISIBLE_SLOTS};
use crate::physics;
use crate::touch::TouchState;
use crate::ui_utils::{get_item_slint, ImageState};
use crate::warp::WarpState;
use crate::AlbumData;

/// Estado relacionado con la navegación y el touch.
#[derive(Clone)]
pub struct InteractionState {
    pub swiper: Rc<RefCell<physics::SwiperPhysics>>,
    pub track_physics: Rc<RefCell<physics::VerticalPhysics>>,
    pub touch: Rc<RefCell<TouchState>>,
    pub warp: Rc<RefCell<WarpState>>,
    pub last_interaction: Rc<RefCell<Instant>>,
    pub albums_pos: Rc<RefCell<(f32, i32)>>,
    pub playlists_pos: Rc<RefCell<(f32, i32)>>,
}

/// Estado relacionado con la biblioteca de música y visualización.
#[derive(Clone)]
pub struct LibraryState {
    pub albums: Rc<RefCell<Vec<api::Album>>>,
    pub playlists: Rc<RefCell<Vec<api::Playlist>>>,
    pub current_mode: Rc<RefCell<api::BrowserMode>>,
    pub image_state: Rc<RefCell<ImageState>>,
    pub track_ids: Rc<RefCell<Vec<String>>>,
    pub model: Rc<VecModel<AlbumData>>,
    pub img_tx: mpsc::Sender<(String, u32, u32, Vec<u8>)>,
    pub last_bg_target_idx: Rc<RefCell<i32>>,
    pub last_bg_update_time: Rc<RefCell<Instant>>,
}

/// Estado relacionado con la reproducción actual.
#[derive(Clone)]
pub struct PlaybackState {
    pub playback_state: Rc<RefCell<String>>,
    pub last_stop_time: Rc<RefCell<Option<Instant>>>,
    pub last_track_id: Rc<RefCell<Option<String>>>,
    // Estado optimista
    pub opt_shuffle: Rc<RefCell<bool>>,
    pub opt_repeat: Rc<RefCell<bool>>,
    pub opt_lock: Rc<RefCell<Instant>>,
    // Sincronización de progreso
    pub last_sync_pos: Rc<RefCell<f32>>,
    pub last_sync_dur: Rc<RefCell<f32>>,
    pub last_sync_time: Rc<RefCell<Instant>>,
}

/// Estado global que agrupa los sub-estados.
#[derive(Clone)]
pub struct AppState {
    pub interaction: InteractionState,
    pub library: LibraryState,
    pub playback: PlaybackState,
    pub api_url: String,
}

impl AppState {
    pub fn new(api_url: String) -> (Self, mpsc::Receiver<(String, u32, u32, Vec<u8>)>) {
        let (img_tx, img_rx) = mpsc::channel();

        let mut swiper = physics::SwiperPhysics::new();
        swiper.spacing = SWIPER_SPACING;

        let albums = Rc::new(RefCell::new(Vec::new()));
        let playlists = Rc::new(RefCell::new(Vec::new()));
        let image_state = Rc::new(RefCell::new(ImageState::default()));
        let current_mode = Rc::new(RefCell::new(api::BrowserMode::Albums));

        let model = Rc::new(VecModel::default());
        for i in 0..VISIBLE_SLOTS {
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
            interaction: InteractionState {
                swiper: Rc::new(RefCell::new(swiper)),
                track_physics: Rc::new(RefCell::new(physics::VerticalPhysics::new())),
                touch: Rc::new(RefCell::new(TouchState::default())),
                warp: Rc::new(RefCell::new(WarpState::None)),
                last_interaction: Rc::new(RefCell::new(Instant::now())),
                albums_pos: Rc::new(RefCell::new((0.0f32, -CENTER_INDEX))),
                playlists_pos: Rc::new(RefCell::new((0.0f32, -CENTER_INDEX))),
            },
            library: LibraryState {
                albums,
                playlists,
                current_mode,
                image_state,
                track_ids: Rc::new(RefCell::new(Vec::new())),
                model,
                img_tx,
                last_bg_target_idx: Rc::new(RefCell::new(-1)),
                last_bg_update_time: Rc::new(RefCell::new(Instant::now())),
            },
            playback: PlaybackState {
                playback_state: Rc::new(RefCell::new(String::from("stop"))),
                last_stop_time: Rc::new(RefCell::new(None)),
                last_track_id: Rc::new(RefCell::new(None)),
                opt_shuffle: Rc::new(RefCell::new(false)),
                opt_repeat: Rc::new(RefCell::new(false)),
                opt_lock: Rc::new(RefCell::new(Instant::now() - Duration::from_secs(5))),
                last_sync_pos: Rc::new(RefCell::new(0.0f32)),
                last_sync_dur: Rc::new(RefCell::new(1.0f32)),
                last_sync_time: Rc::new(RefCell::new(Instant::now())),
            },
            api_url,
        };

        (state, img_rx)
    }
}
