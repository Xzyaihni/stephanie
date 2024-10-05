#![allow(clippy::suspicious_else_formatting)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::new_without_default)]
#![allow(clippy::needless_update)]
// the fact that i can derive it is a coincidence
#![allow(clippy::derivable_impls)]
// this is so stupid
#![allow(clippy::len_without_is_empty)]

use std::{process, fmt::Display};

use yanyaengine::{App, ShadersContainer, ShadersInfo};

pub use app::{common, server, client, ProgramShaders};

use app::AppInfo;

use common::world::TILE_SIZE;
pub use common::{debug_env, is_debug_env};
pub mod debug_config;

mod app;


mod default_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/default.vert"
    }
}

mod default_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/default.frag"
    }
}

mod world_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/world.frag"
    }
}

mod shadow_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/shadow.vert"
    }
}

mod shadow_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/shadow.frag"
    }
}

mod ui_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/ui.vert"
    }
}

mod ui_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/ui.frag"
    }
}

pub const LOG_PATH: &str = "log.txt";

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

    let mut shaders = ShadersContainer::new();

    let default_vertex = |device|
    {
        default_vertex::load(device).unwrap().specialize(
            [(0, TILE_SIZE.into())].into_iter().collect()
        )
    };

    let default_shader = shaders.push(ShadersInfo::new(
        default_vertex,
        default_fragment::load
    ));

    let world_shader = shaders.push(ShadersInfo::new(
        default_vertex,
        world_fragment::load
    ));

    let shadow_shader = shaders.push(ShadersInfo::new(
        shadow_vertex::load,
        shadow_fragment::load
    ));

    let ui_shader = shaders.push(ShadersInfo::new(
        ui_vertex::load,
        ui_fragment::load
    ));

    let init = AppInfo{
        shaders: ProgramShaders{
            default: default_shader,
            world: world_shader,
            shadow: shadow_shader,
            ui: ui_shader
        }
    };

    App::<app::App>::new()
        .with_title("stey funy")
        .with_textures_path("textures/normal")
        .with_icon("icon.png")
        .with_shaders(shaders, default_shader)
        .with_app_init(Some(init))
        .without_multisampling()
        .with_clear_color([0.831, 0.941, 0.988])
        .run();
}
