pub mod state;

use slint::{ComponentHandle, Model, VecModel, Image};
use std::rc::Rc;
use crate::AppWindow;
use self::state::AppState;
use crate::config::*;

pub struct Application {
    pub ui: AppWindow,
    pub state: AppState,
}

impl Application {
    pub fn init() -> Result<(Self, std::sync::mpsc::Receiver<(String, u32, u32, Vec<u8>)>), slint::PlatformError> {
        let ui = AppWindow::new()?;
        let api_url = std::env::var("API_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());
        
        let (state, img_rx) = AppState::new(api_url);

        // Setup UI initial state
        ui.set_visible_items(state.library.model.clone().into());
        let x_pos: Vec<f32> = (-CENTER_INDEX..=CENTER_INDEX)
            .map(|i| CENTER_X + (i as f32) * state.interaction.swiper.borrow().spacing)
            .collect();
        ui.set_x_positions(Rc::new(VecModel::from(x_pos)).into());
        ui.set_center_index(CENTER_INDEX);

        Ok((Self { ui, state }, img_rx))
    }



    pub fn run(self) -> Result<(), slint::PlatformError> {
        self.ui.run()
    }
}
