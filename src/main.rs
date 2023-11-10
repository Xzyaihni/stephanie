use yanyaengine::{App, ShadersInfo, ShaderItem};

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
    App::<app::App>::new()
        .with_title("stey funy")
        .with_textures_path("textures/normal")
        .with_icon("icon.png")
        // i genuinely cant think of a less cancer way of taking shaders
        .with_shaders([ShadersInfo::new(
            ShaderItem::new(Box::new(|device| default_vertex::load(device))),
            ShaderItem::new(Box::new(|device| default_fragment::load(device)))
        )])
        .with_clear_color([0.0, 0.0, 0.0])
        .run();
}
