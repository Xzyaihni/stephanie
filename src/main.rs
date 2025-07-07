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
