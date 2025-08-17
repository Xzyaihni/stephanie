use std::sync::Arc;

use parking_lot::RwLock;

use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::{
    game_object::*,
    object::Model,
    DefaultModel,
    ObjectFactory,
    TransformContainer,
    Transform,
    SolidObject,
    ObjectVertex
};

use crate::{
    client::VisibilityChecker,
    common::{Pos3, Entity, ServerToClient, world::TilePos}
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

impl Light
{
    pub fn build(
        self,
        position: Vector3<f32>,
        object_factory: &ObjectFactory,
        square: Arc<RwLock<Model>>
    ) -> ClientLight
    {
        let transform = Transform{
            position,
            ..Default::default()
        };

        let object = object_factory.create_solid(
            square,
            transform
        );

        let mut this = ClientLight{light: self, occluded: false, position, object};

        this.light_modified();

        this
    }
}

#[derive(Debug)]
pub struct ClientLight
{
    light: Light,
    pub occluded: bool,
    position: Vector3<f32>,
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
        self.position = position;

        let z = TilePos::from(position).offset(Pos3::new(0, 0, -1)).position().z;

        self.object.set_position(Vector3::new(position.x, position.y, z));

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
        self.visibility_checker_with(self.position)
    }

    pub fn visibility_checker_with(&self, position: Vector3<f32>) -> VisibilityChecker
    {
        VisibilityChecker{
            world_position: TilePos::from(position),
            size: self.scale(),
            position
        }
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
        create_info: &mut UpdateBuffersInfo
    ) -> ClientLight
    {
        let assets = create_info.partial.assets.lock();
        let object_factory = &create_info.partial.object_factory;

        self.build(
            transform().position,
            object_factory,
            assets.model(assets.default_model(DefaultModel::Square)).clone()
        )
    }
}
