//! Utilidades de UI: carga de imágenes async y construcción de datos para Slint.
//!
//! `ImageState` es la caché de imágenes (evita recargas). Las imágenes se cargan
//! en threads separados y llegan por canal mpsc al timer tick.

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

pub trait SlintItemConverter {
    fn to_slint(
        &self,
        img_state: &mut ImageState,
        tx: &mpsc::Sender<(String, u32, u32, Vec<u8>)>,
    ) -> AlbumData;
}

pub fn spawn_image_loader(path: String, tx: mpsc::Sender<(String, u32, u32, Vec<u8>)>) {
    let p_clone = path.clone();
    std::thread::spawn(move || {
        log::debug!("Image: Loading from disk: {}", p_clone);
        match image::open(&p_clone) {
            Ok(img) => {
                // NOTA FUTURO: El redimensionado en tiempo real se ha deshabilitado porque
                // el backend ya provee archivos '.album_thumb.jpg' que vienen pre-convertidos.
                // Esto ahorra ciclos de CPU valiosos en la Raspberry Pi Zero 2W.
                // Si en el futuro se usan imágenes originales grandes, descomentar la línea de 'thumbnail'.
                
                // let thumb = img.thumbnail(256, 256);
                let rgba = img.into_rgba8(); // Usamos 'img' directamente en lugar de 'thumb'
                let (w, h) = (rgba.width(), rgba.height());
                let pixels = rgba.into_raw();
                let _ = tx.send((p_clone, w, h, pixels));
            },
            Err(e) => {
                log::error!("Image: Failed to open {}: {}", p_clone, e);
            }
        }
    });
}

impl SlintItemConverter for api::Album {
    fn to_slint(
        &self,
        img_state: &mut ImageState,
        tx: &mpsc::Sender<(String, u32, u32, Vec<u8>)>,
    ) -> AlbumData {
        let path = self.cover_thumb.as_ref().or(self.cover.as_ref());
        let cover = if let Some(path) = path {
            if let Some(img) = img_state.cache.get(path) {
                img.clone()
            } else {
                if !img_state.loading.contains(path) {
                    img_state.loading.insert(path.clone());
                    spawn_image_loader(path.clone(), tx.clone());
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
        tx: &mpsc::Sender<(String, u32, u32, Vec<u8>)>,
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
                    spawn_image_loader(path.clone(), tx.clone());
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
        albums[idx as usize].to_slint(img_state, tx)
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
        playlists[idx as usize].to_slint(img_state, tx)
    }
}

pub fn preload_neighborhood(
    mode: &api::BrowserMode,
    albums: &[api::Album],
    playlists: &[api::Playlist],
    img_state: &mut ImageState,
    tx: &mpsc::Sender<(String, u32, u32, Vec<u8>)>,
    current_offset: i32,
) {
    use crate::config::{PRELOAD_BACKWARD, PRELOAD_FORWARD};
    let range = -PRELOAD_BACKWARD..=PRELOAD_FORWARD; 
    let mut paths = HashSet::new();

    for i in range {
        let abs_idx = current_offset + i;
        if *mode == api::BrowserMode::Albums {
            let n = albums.len();
            if n > 0 {
                let album = &albums[abs_idx.rem_euclid(n as i32) as usize];
                if let Some(p) = album.cover_thumb.as_ref().or(album.cover.as_ref()) {
                    paths.insert(p.clone());
                }
            }
        } else {
            let n = playlists.len();
            if n > 0 {
                let pl = &playlists[abs_idx.rem_euclid(n as i32) as usize];
                if let Some(id) = pl.id.as_ref() {
                    paths.insert(format!("../data/playlists/covers/cover_{}.jpg", id));
                }
            }
        }
    }

    for path in paths {
        if !img_state.cache.contains_key(&path) && !img_state.loading.contains(&path) {
            img_state.loading.insert(path.clone());
            spawn_image_loader(path.clone(), tx.clone());
        }
    }
}

pub fn cleanup_cache(
    img_state: &mut ImageState,
    current_lib_offset: i32,
    mode: &api::BrowserMode,
    albums: &[api::Album],
    playlists: &[api::Playlist],
) {
    // Solo actuamos si la caché supera un threshold razonable para evitar ciclos constantes.
    // 100 imágenes son ~25MB en RAW, muy seguro incluso en la Pi Zero 2W.
    if img_state.cache.len() < 100 {
        return;
    }

    // Identificamos el set de paths que DEBEMOS mantener (ventana de pre-carga + margen)
    use crate::config::{PRELOAD_BACKWARD, PRELOAD_FORWARD};
    let mut keep_paths = std::collections::HashSet::new();
    
    // Definimos un rango un poco más amplio que la precarga para evitar oscilaciones
    let range = -(PRELOAD_BACKWARD + 10)..=(PRELOAD_FORWARD + 10);
    
    for i in range {
        let abs_idx = current_lib_offset + i;
        if *mode == api::BrowserMode::Albums {
            let n = albums.len();
            if n > 0 {
                let album = &albums[abs_idx.rem_euclid(n as i32) as usize];
                if let Some(p) = album.cover_thumb.as_ref().or(album.cover.as_ref()) {
                    keep_paths.insert(p.clone());
                }
            }
        } else {
            let n = playlists.len();
            if n > 0 {
                let pl = &playlists[abs_idx.rem_euclid(n as i32) as usize];
                if let Some(id) = pl.id.as_ref() {
                    keep_paths.insert(format!("../data/playlists/covers/cover_{}.jpg", id));
                }
            }
        }
    }

    let before = img_state.cache.len();
    img_state.cache.retain(|path, _| {
        // Reservamos las imágenes de sistema (assets) y las del vecindario actual
        path.starts_with("assets/") || keep_paths.contains(path)
    });
    
    let after = img_state.cache.len();
    if before != after {
        log::info!("Cache: Cleaned up {} old images ({} -> {}). Freeing memory.", before - after, before, after);
    }
}

pub fn go_to_selector(ui: &crate::AppWindow) {
    ui.set_current_screen(crate::ScreenState::Selector);
}

pub fn open_track_picker(ui: &crate::AppWindow) {
    ui.set_current_screen(crate::ScreenState::TrackPicker);
}
