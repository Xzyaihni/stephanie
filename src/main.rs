use yanyaengine::{App, ShadersContainer, ShadersInfo};

pub use app::{common, server, client};

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

fn main()
{
    let mut shaders = ShadersContainer::new();

    shaders.push(ShadersInfo::new(
        default_vertex::load,
        default_fragment::load
    ));

    App::<app::App>::new()
        .with_title("stey funy")
        .with_textures_path("textures/normal")
        .with_icon("icon.png")
        .with_shaders(shaders)
        .without_multisampling()
        .with_clear_color([0.0, 0.0, 0.0])
        .run();
}
