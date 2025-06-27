use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::{game_object::*, TransformContainer, Transform, SolidObject, ObjectVertex};

use crate::{
    client::{VisibilityChecker, RenderCreateInfo},
    common::{Entity, ServerToClient}
};


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Light
{
    pub source: Option<Entity>,
    pub strength: f32
}

impl Default for Light
{
    fn default() -> Self
    {
        Self{source: None, strength: 0.0}
    }
}

#[derive(Debug)]
pub struct ClientLight
{
    light: Light,
    pub occluded: bool,
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
        let scale = self.scale();
        self.object.set_scale(scale);
    }

    pub fn scale(&self) -> Vector3<f32>
    {
        let scale = self.light.strength;
        Vector3::new(scale, scale, 1.0)
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

    pub fn visible_with(&self, visibility: &VisibilityChecker, transform: &Transform) -> bool
    {
        self.is_visible() && visibility.visible_sphere(&Transform{
            position: transform.position,
            scale: self.scale(),
            ..Default::default()
        })
    }

    pub fn visibility_checker(&self) -> VisibilityChecker
    {
        self.visibility_checker_with(*self.object.position())
    }

    pub fn visibility_checker_with(&self, position: Vector3<f32>) -> VisibilityChecker
    {
        VisibilityChecker::new(self.scale(), position)
    }

    pub fn is_visible(&self) -> bool
    {
        !self.occluded && self.light.strength > 0.0
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

        let mut this = ClientLight{light: self, occluded: false, object};

        this.light_modified();

        this
    }
}
