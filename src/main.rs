#![allow(clippy::suspicious_else_formatting)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::new_without_default)]
#![allow(clippy::needless_update)]
// the fact that i can derive it is a coincidence
#![allow(clippy::derivable_impls)]
// this is so stupid
#![allow(clippy::len_without_is_empty)]
// collapsed ones r way less readable in most cases :/
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::single_match)]
#![allow(clippy::needless_lifetimes)]
// skill issue
#![allow(clippy::type_complexity)]
// ITS MORE DESCRIPTIVE OF WUT IT IS
#![allow(clippy::let_and_return)]
// consistency????????
#![allow(clippy::excessive_precision)]

use std::{process, fmt::Display};

use nalgebra::Vector3;

use yanyaengine::App;

pub use app::{common, server, client, ProgramShaders};

use app::AppInfo;

pub use common::{debug_env, is_debug_env};

pub use shaders::{DARKEN, SHADOW_COLOR};

pub mod debug_config;

mod app;
mod rendering;
mod shaders;


pub const LOG_PATH: &str = "log.txt";
pub const LONGEST_FRAME: f64 = 1.0 / 20.0;

pub const BACKGROUND_COLOR: Vector3<f32> = Vector3::new(0.831, 0.941, 0.988);

pub fn complain(message: impl Display) -> !
{
    eprintln!("{message}");

    process::exit(1)
}

/*#[link(name = "floathelper")]
extern "C"
{
    fn float_excepts();
}*/

fn main()
{
    // unsafe{ float_excepts() };

    let shaders::ShadersCreated{shaders, group} = shaders::create();

    let init = AppInfo{
        shaders: group
    };

    let rendering = rendering::create();

    App::<app::App>::new()
        .with_title("stey funy")
        .with_textures_path("textures/normal")
        .with_icon("icon.png")
        .with_shaders(shaders)
        .with_app_init(Some(init))
        .with_rendering(rendering)
        .run();
}
