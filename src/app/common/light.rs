use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::{game_object::*, TransformContainer, Transform, SolidObject, ObjectVertex};

use crate::{
    client::RenderCreateInfo,
    common::ServerToClient
};


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Light
{
    pub strength: f32
}

impl Default for Light
{
    fn default() -> Self
    {
        Self{strength: 0.0}
    }
}

#[derive(Debug)]
pub struct ClientLight
{
    light: Light,
    object: SolidObject<ObjectVertex>
}

impl ClientLight
{
    pub fn modify_light(&mut self, f: impl FnOnce(&mut Light))
    {
        f(&mut self.light);

        self.light_modified();
    }

    fn light_modified(&mut self)
    {
        let scale = self.light.strength;
        self.object.set_scale(Vector3::new(scale, scale, 1.0));
    }

    pub fn update_buffers(&mut self, info: &mut UpdateBuffersInfo, position: Vector3<f32>)
    {
        self.object.set_position(position);
        self.object.update_buffers(info);
    }

    pub fn draw(&self, info: &mut DrawInfo)
    {
        self.object.draw(info);
    }

    pub fn is_visible(&self) -> bool
    {
        self.light.strength > 0.0
    }
}

impl ServerToClient<ClientLight> for Light
{
    fn server_to_client(
        self,
        transform: impl FnOnce() -> Transform,
        create_info: &mut RenderCreateInfo
    ) -> ClientLight
    {
        let transform = Transform{
            position: transform().position,
            ..Default::default()
        };

        let assets = create_info.object_info.partial.assets.lock();
        let object = create_info.object_info.partial.object_factory.create_solid(
            assets.model(create_info.ids.square).clone(),
            transform
        );

        let mut this = ClientLight{light: self, object};

        this.light_modified();

        this
    }
}
