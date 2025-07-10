use yanyaengine::App;

use stephanie::app::{self, AppInfo};

pub use stephanie::common::{debug_env, is_debug_env};

mod rendering;
mod shaders;


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
