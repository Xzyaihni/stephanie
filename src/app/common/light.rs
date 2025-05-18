use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::{ObjectInfo, TransformContainer, Transform, Object};

use crate::{
    client::RenderCreateInfo,
    common::ServerToClient
};


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Light
{
    pub strength: f32
}

#[derive(Debug)]
pub struct ClientLight
{
    light: Light,
    object: Object
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

    pub fn update_buffers(&mut self, position: Vector3<f32>)
    {
        self.object.set_position(position);
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
        let info = ObjectInfo{
            model: assets.model(create_info.ids.square).clone(),
            texture: assets.texture(create_info.ids.light_texture).clone(),
            transform
        };

        let object = create_info.object_info.partial.object_factory.create(info);

        let mut this = ClientLight{light: self, object};

        this.light_modified();

        this
    }
}
