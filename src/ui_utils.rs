//! Utilidades de UI: carga de imágenes async y construcción de datos para Slint.
//!
//! `ImageState` es la caché de imágenes (evita recargas). Las imágenes se cargan
//! via `ImageLoader` (workers persistentes) y llegan por canal mpsc al timer tick.

use crate::api;
use crate::loader::ImageLoader;
use slint::Image;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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

pub trait SlintItemConverter {
    fn to_slint(
        &self,
        img_state: &mut ImageState,
        loader: &Arc<ImageLoader>,
    ) -> AlbumData;
}

impl SlintItemConverter for api::Album {
    fn to_slint(
        &self,
        img_state: &mut ImageState,
        loader: &Arc<ImageLoader>,
    ) -> AlbumData {
        let path = self.cover_thumb.as_ref().or(self.cover.as_ref());
        let cover = if let Some(path) = path {
            if let Some(img) = img_state.cache.get(path) {
                img.clone()
            } else {
                if !img_state.loading.contains(path) {
                    img_state.loading.insert(path.clone());
                    loader.enqueue(path.clone());
                }
                Image::default()
            }
        } else {
            Image::default()
        };

        let artist = self.album_artist.clone().unwrap_or_default();
        AlbumData {
            title: self.title.clone().into(),
            artist: artist.clone().into(),
            album_title: self.title.clone().into(),
            album_artist: artist.into(),
            cover,
        }
    }
}

impl SlintItemConverter for api::Playlist {
    fn to_slint(
        &self,
        img_state: &mut ImageState,
        loader: &Arc<ImageLoader>,
    ) -> AlbumData {
        // La portada de las playlists está en una ubicación específica según el ID
        let path = if let Some(id) = self.id.as_ref() {
            let p = format!("../data/playlists/covers/cover_{}.jpg", id);
            log::info!("Playlist: Requesting cover: {}", p);
            Some(p)
        } else {
            log::warn!("Playlist: ID missing for: {}", self.name);
            None
        };

        let cover = if let Some(path) = path {
            if let Some(img) = img_state.cache.get(&path) {
                img.clone()
            } else {
                if !img_state.loading.contains(&path) {
                    img_state.loading.insert(path.clone());
                    loader.enqueue(path.clone());
                }
                Image::default()
            }
        } else {
            Image::default()
        };

        AlbumData {
            title: self.name.clone().into(),
            artist: format!("{} canciones", self.track_count).into(),
            album_title: self.name.clone().into(),
            album_artist: "Playlist".into(),
            cover,
        }
    }
}

pub fn get_item_slint(
    mode: &api::BrowserMode,
    albums: &[api::Album],
    playlists: &[api::Playlist],
    img_state: &mut ImageState,
    loader: &Arc<ImageLoader>,
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
        albums[idx as usize].to_slint(img_state, loader)
    } else {
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
        playlists[idx as usize].to_slint(img_state, loader)
    }
}

/// Encola todas las portadas en el rango `[center - half, center + half]`.
pub fn enqueue_preload_range(
    center: i32,
    half: i32,
    mode: &api::BrowserMode,
    albums: &[api::Album],
    playlists: &[api::Playlist],
    img_state: &mut ImageState,
    loader: &Arc<ImageLoader>,
) {
    let start = center - half;
    let end = center + half;
    for abs_idx in start..=end {
        let path = if *mode == api::BrowserMode::Albums {
            let n = albums.len() as i32;
            if n > 0 {
                let album = &albums[abs_idx.rem_euclid(n) as usize];
                album.cover_thumb.as_ref().or(album.cover.as_ref()).cloned()
            } else {
                None
            }
        } else {
            let n = playlists.len() as i32;
            if n > 0 {
                let pl = &playlists[abs_idx.rem_euclid(n) as usize];
                pl.id.as_ref().map(|id| format!("../data/playlists/covers/cover_{}.jpg", id))
            } else {
                None
            }
        };
        if let Some(path) = path {
            if !img_state.cache.contains_key(&path) && !img_state.loading.contains(&path) {
                img_state.loading.insert(path.clone());
                loader.enqueue(path.clone());
            }
        }
    }
}

pub fn cleanup_cache(
    img_state: &mut ImageState,
    window_center: i32,
    mode: &api::BrowserMode,
    albums: &[api::Album],
    playlists: &[api::Playlist],
) {
    use crate::config::PRELOAD_WINDOW_HALF;
    let margin = 10;
    let keep_range = PRELOAD_WINDOW_HALF + margin;
    let start = window_center - keep_range;
    let end = window_center + keep_range;

    let mut keep_paths = std::collections::HashSet::new();
    for abs_idx in start..=end {
        if *mode == api::BrowserMode::Albums {
            let n = albums.len() as i32;
            if n > 0 {
                let album = &albums[abs_idx.rem_euclid(n) as usize];
                if let Some(p) = album.cover_thumb.as_ref().or(album.cover.as_ref()) {
                    keep_paths.insert(p.clone());
                }
            }
        } else {
            let n = playlists.len() as i32;
            if n > 0 {
                let pl = &playlists[abs_idx.rem_euclid(n) as usize];
                if let Some(id) = pl.id.as_ref() {
                    keep_paths.insert(format!("../data/playlists/covers/cover_{}.jpg", id));
                }
            }
        }
    }

    let before = img_state.cache.len();
    img_state.cache.retain(|path, _| {
        path.starts_with("assets/") || keep_paths.contains(path)
    });

    let after = img_state.cache.len();
    if before != after {
        log::info!("Cache: Cleaned up {} old images ({} -> {}). Freeing memory.", before - after, before, after);
    }
}

/// Encuentra qué slot visible (0..VISIBLE_SLOTS) corresponde a una ruta de portada.
/// Retorna None si la imagen no está en ningún slot visible actual.
pub fn find_slot_for_cover_path(
    mode: &api::BrowserMode,
    albums: &[api::Album],
    playlists: &[api::Playlist],
    path: &str,
    lib_offset: i32,
) -> Option<i32> {
    let n_albums = albums.len() as i32;
    let n_playlists = playlists.len() as i32;
    for i in 0..crate::config::VISIBLE_SLOTS {
        let abs_idx = lib_offset + i;
        let cover_path = if *mode == api::BrowserMode::Albums {
            if n_albums > 0 {
                let album = &albums[abs_idx.rem_euclid(n_albums) as usize];
                album.cover_thumb.as_ref().or(album.cover.as_ref()).cloned()
            } else {
                None
            }
        } else {
            if n_playlists > 0 {
                let pl = &playlists[abs_idx.rem_euclid(n_playlists) as usize];
                pl.id.as_ref().map(|id| format!("../data/playlists/covers/cover_{}.jpg", id))
            } else {
                None
            }
        };
        if cover_path.as_deref() == Some(path) {
            return Some(i);
        }
    }
    None
}

pub fn go_to_selector(ui: &crate::AppWindow) {
    ui.set_current_screen(crate::ScreenState::Selector);
}

pub fn open_track_picker(ui: &crate::AppWindow) {
    ui.set_current_screen(crate::ScreenState::TrackPicker);
}
