use std::fs;

use crate::{
    common::with_error,
    client::{
        Control,
        KeyMapping,
        game_state::default_bindings
    }
};

use vulkano::swapchain::PresentMode;

use serde::{Serialize, Deserialize};


pub const DEFAULT_SETTINGS_PATH: &str = "settings.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FrameLimit
{
    RefreshRate,
    Unlimited
}

impl Default for FrameLimit
{
    fn default() -> Self
    {
        Self::RefreshRate
    }
}

impl FrameLimit
{
    pub fn as_present_mode(self) -> PresentMode
    {
        match self
        {
            Self::RefreshRate => PresentMode::Fifo,
            Self::Unlimited => PresentMode::Immediate
        }
    }

    pub fn as_str(self) -> &'static str
    {
        match self
        {
            Self::RefreshRate => "vsync",
            Self::Unlimited => "unlimited"
        }
    }

    pub fn next(self) -> Self
    {
        match self
        {
            Self::RefreshRate => Self::Unlimited,
            Self::Unlimited => Self::RefreshRate
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct GameSettings
{
    pub key_bindings: Vec<(KeyMapping, Control)>,
    pub frame_limit: FrameLimit
}

impl Default for GameSettings
{
    fn default() -> Self
    {
        Self{
            key_bindings: default_bindings(),
            frame_limit: FrameLimit::default()
        }
    }
}

fn try_load_settings_config() -> Option<GameSettings>
{
    let path = DEFAULT_SETTINGS_PATH;

    if !fs::exists(path).unwrap_or(true)
    {
        return None;
    }

    let settings_s = with_error(fs::read_to_string(path))?;

    with_error(serde_json::from_str(&settings_s))
}

pub fn load_settings_config() -> GameSettings
{
    try_load_settings_config().unwrap_or_default()
}
