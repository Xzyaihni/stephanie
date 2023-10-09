use yanyaengine::App;

pub use app::{common, server, client};

mod app;


fn main()
{
    App::<app::App>::new()
        .with_title("stey funy")
        .with_textures_path("textures/normal")
        .with_icon("icon.png")
        .with_clear_color([0.0, 0.0, 0.0])
        .run();
}
