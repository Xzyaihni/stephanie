use yanyaengine::App;

pub use app::{common, server, client};

mod app;


fn main()
{
    App::<app::App>::new()
        .with_title("very cool new game, nobody ever created something like this")
        .with_textures_path("textures")
        .with_icon("icon.png")
        .with_clear_color([0.0, 0.0, 0.0])
        .run();
}
