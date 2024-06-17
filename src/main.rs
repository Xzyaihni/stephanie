#![allow(clippy::suspicious_else_formatting)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::new_without_default)]
#![allow(clippy::needless_update)]
// this is so stupid
#![allow(clippy::len_without_is_empty)]

use yanyaengine::{App, ShadersContainer, ShadersInfo};

pub use app::{common, server, client, ProgramShaders};
use app::AppInfo;

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

fn main()
{
    let mut shaders = ShadersContainer::new();

    let default_shader = shaders.push(ShadersInfo::new(
        default_vertex::load,
        default_fragment::load
    ));

    let world_shader = shaders.push(ShadersInfo::new(
        default_vertex::load,
        world_fragment::load
    ));

    let init = AppInfo{
        shaders: ProgramShaders{
            default: default_shader,
            world: world_shader
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
