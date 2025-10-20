use yanyaengine::{game_object::*, Control};

use crate::client::ClientInfo;


pub struct MainMenu
{
    info: ClientInfo
}

impl MainMenu
{
    pub fn new(
        address: String,
        host: bool
    ) -> Self
    {
        let info = ClientInfo{
            address,
            name: "stephanie".to_owned(),
            host,
            debug: false
        };

        Self{
            info
        }
    }


    pub fn update<'a>(
        &mut self,
        partial_info: UpdateBuffersPartialInfo<'a>,
        dt: f32
    ) -> Option<(UpdateBuffersPartialInfo<'a>, ClientInfo)>
    {
        Some((partial_info, self.info.clone()))
    }

    pub fn input(&mut self, control: Control)
    {
    }

    pub fn mouse_move(&mut self, position: (f64, f64))
    {
    }

    pub fn draw(&mut self, info: DrawInfo)
    {
    }

    pub fn resize(&mut self, aspect: f32)
    {
    }
}
