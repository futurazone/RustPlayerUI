use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Track {
    pub track_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub duration: Option<f32>,
    pub track_number: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Album {
    pub title: String,
    pub album_artist: Option<String>,
    pub cover: Option<String>,
    pub cover_thumb: Option<String>,
    pub tracks: Option<Vec<Track>>,
}

pub fn play_track(api_url: &str, track_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("API: Playing track {} at {}", track_id, api_url);
    let _ = reqwest::blocking::get(format!("{}/play/?track_id={}", api_url, track_id))?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Playlist {
    pub id: Option<String>,
    pub name: String,
    pub track_count: i32,
    pub cover: Option<String>,
    pub covers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryResponse {
    pub albums: Vec<Album>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum BrowserMode {
    Albums,
    Playlists,
}

pub fn fetch_real_albums(api_url: &str) -> Result<Vec<Album>, Box<dyn std::error::Error>> {
    log::info!("API: Fetching REAL albums from {}/list", api_url);
    let resp = reqwest::blocking::get(format!("{}/list", api_url))?;
    let lib: LibraryResponse = resp.json()?;
    log::info!("API: Loaded {} albums", lib.albums.len());
    Ok(lib.albums)
}

pub fn fetch_real_playlists(api_url: &str) -> Result<Vec<Playlist>, Box<dyn std::error::Error>> {
    log::info!("API: Fetching REAL playlists from {}/playlists", api_url);
    let resp = reqwest::blocking::get(format!("{}/playlists", api_url))?;
    let playlists: Vec<Playlist> = resp.json()?;
    log::info!("API: Loaded {} playlists", playlists.len());
    Ok(playlists)
}
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PlayerStatus {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub paused: Option<bool>,
    pub position: Option<f32>,
    pub duration: Option<f32>,
    pub shuffle: Option<bool>,
    pub repeat: Option<bool>,
    pub state: Option<String>, // "play", "pause", "stop"
}

pub fn get_real_status(api_url: &str) -> Result<PlayerStatus, Box<dyn std::error::Error>> {
    let resp = reqwest::blocking::get(format!("{}/status", api_url))?;
    let status: PlayerStatus = resp.json()?;
    Ok(status)
}

pub fn send_player_command(api_url: &str, command: &str) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("API: Sending command {} to {}", command, api_url);
    let client = reqwest::blocking::Client::new();
    let _ = client.post(format!("{}/{}", api_url, command)).send()?;
    Ok(())
}

#[derive(Serialize)]
struct QueueRequest<'a> {
    tracks: &'a Vec<String>,
}

pub fn send_queue(api_url: &str, track_ids: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!(
        "API: Sending queue with {} tracks to {}",
        track_ids.len(),
        api_url
    );
    let client = reqwest::blocking::Client::new();
    let payload = QueueRequest { tracks: &track_ids };
    let _ = client
        .post(format!("{}/queue", api_url))
        .json(&payload)
        .send()?;
    Ok(())
}
