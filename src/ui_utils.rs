use crate::api;
use slint::Image;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;

// Re-importing generated types from crate root
use crate::AlbumData;

pub struct ImageState {
    pub cache: HashMap<String, Image>,
    pub loading: HashSet<String>,
}

impl Default for ImageState {
    fn default() -> Self {
        Self {
            cache: HashMap::new(),
            loading: HashSet::new(),
        }
    }
}

pub fn get_item_slint(
    mode: &api::BrowserMode,
    albums: &[api::Album],
    playlists: &[api::Playlist],
    img_state: &mut ImageState,
    tx: &mpsc::Sender<(String, u32, u32, Vec<u8>)>,
    abs_idx: i32,
) -> AlbumData {
    if *mode == api::BrowserMode::Albums {
        let n = albums.len() as i32;
        if n == 0 {
            return AlbumData {
                title: "Sin álbumes".into(),
                artist: "".into(),
                album_title: "".into(),
                album_artist: "".into(),
                cover: Image::default(),
            };
        }
        let idx = ((abs_idx % n) + n) % n;
        let album = &albums[idx as usize];

        let path_to_load = album.cover_thumb.as_ref().or(album.cover.as_ref());

        let cover = if let Some(path) = path_to_load {
            if let Some(img) = img_state.cache.get(path) {
                img.clone()
            } else {
                if !img_state.loading.contains(path) {
                    img_state.loading.insert(path.clone());
                    let path_clone = path.clone();
                    let tx_clone = tx.clone();
                    std::thread::spawn(move || {
                        if let Ok(img) = image::open(&path_clone) {
                            let rgba = img.into_rgba8();
                            let width = rgba.width();
                            let height = rgba.height();
                            let pixels = rgba.into_raw();
                            let _ = tx_clone.send((path_clone, width, height, pixels));
                        }
                    });
                }
                Image::default()
            }
        } else {
            Image::default()
        };

        let artist = album.album_artist.clone().unwrap_or_default();
        AlbumData {
            title: album.title.clone().into(),
            artist: artist.clone().into(),
            album_title: album.title.clone().into(),
            album_artist: artist.into(),
            cover,
        }
    } else {
        // Playlists
        let n = playlists.len() as i32;
        if n == 0 {
            return AlbumData {
                title: "Sin listas".into(),
                artist: "".into(),
                album_title: "".into(),
                album_artist: "".into(),
                cover: Image::default(),
            };
        }
        let idx = ((abs_idx % n) + n) % n;
        let pl = &playlists[idx as usize];

        // PRIORIDAD: 1. pl.cover (portada dedicada) -> 2. pl.covers[0] (primera canción)
        let path_to_load = pl
            .cover
            .as_ref()
            .or_else(|| pl.covers.as_ref().and_then(|c| c.get(0)));

        let cover = if let Some(path) = path_to_load {
            if let Some(img) = img_state.cache.get(path) {
                img.clone()
            } else {
                if !img_state.loading.contains(path) {
                    img_state.loading.insert(path.clone());
                    let path_clone = path.clone();
                    let tx_clone = tx.clone();
                    std::thread::spawn(move || {
                        if let Ok(img) = image::open(&path_clone) {
                            let rgba = img.into_rgba8();
                            let width = rgba.width();
                            let height = rgba.height();
                            let pixels = rgba.into_raw();
                            let _ = tx_clone.send((path_clone, width, height, pixels));
                        }
                    });
                }
                Image::default()
            }
        } else {
            Image::default()
        };

        AlbumData {
            title: pl.name.clone().into(),
            artist: format!("{} canciones", pl.track_count).into(),
            album_title: pl.name.clone().into(),
            album_artist: "Playlist".into(),
            cover,
        }
    }
}

pub fn go_to_selector(ui: &crate::AppWindow) {
    log::info!("Navigation: Go to Selector");
    ui.set_current_screen(crate::ScreenState::Selector);
}

pub fn open_track_picker(ui: &crate::AppWindow) {
    log::info!("Navigation: Open Track Picker");
    ui.set_current_screen(crate::ScreenState::TrackPicker);
}
