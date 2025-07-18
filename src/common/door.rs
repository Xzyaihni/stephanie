use std::f32;

use strum::{EnumString, IntoStaticStr};

use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    some_or_return,
    rotate_point_z_3d,
    collider::*,
    watcher::*,
    Entity,
    Occluder,
    entity::ClientEntities,
    world::{TILE_SIZE, TileRotation}
};


#[derive(Debug, Clone, Copy, EnumString, IntoStaticStr, Serialize, Deserialize)]
#[strum(ascii_case_insensitive)]
pub enum DoorMaterial
{
    Metal,
    Wood
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Door
{
    position: Vector3<f32>,
    rotation: TileRotation,
    material: DoorMaterial,
    width: u32,
    open: bool
}

impl Door
{
    pub fn new(
        position: Vector3<f32>,
        rotation: TileRotation,
        material: DoorMaterial,
        width: u32
    ) -> Self
    {
        Self{position, rotation, material, width, open: false}
    }

    pub fn is_open(&self) -> bool
    {
        self.open
    }

    fn door_rotation(&self) -> f32
    {
        self.rotation.to_angle() + f32::consts::PI
    }

    pub fn set_open(
        &mut self,
        entities: &ClientEntities,
        entity: Entity,
        opener: Entity,
        state: bool
    )
    {
        if self.open != state
        {
            self.open = state;

            if let Some(parent) = entities.parent(entity)
            {
                let visible_door = parent.entity();
                if let Some(mut lazy) = entities.lazy_transform_mut(visible_door)
                {
                    let angle = if self.open
                    {
                        let opener_position = some_or_return!(entities.transform(opener)).position;
                        let this_position = self.position;

                        let flip = match self.rotation
                        {
                            TileRotation::Left => opener_position.y < this_position.y,
                            TileRotation::Right => opener_position.y > this_position.y,
                            TileRotation::Down => opener_position.x > this_position.x,
                            TileRotation::Up => opener_position.x < this_position.x
                        };

                        if flip
                        {
                            f32::consts::FRAC_PI_2
                        } else
                        {
                            -f32::consts::FRAC_PI_2
                        }
                    } else
                    {
                        0.0
                    };

                    lazy.set_origin_rotation(angle);
                }

                let collider = self.door_collider();
                let occluder = self.door_occluder();

                let mut setter = entities.lazy_setter.borrow_mut();
                setter.set_occluder(visible_door, occluder);

                if self.open
                {
                    setter.set_collider(visible_door, collider);
                } else
                {
                    if let Some(mut watchers) = entities.watchers_mut(visible_door)
                    {
                        let collider_watcher = Watcher{
                            kind: WatcherType::RotationDistance{
                                from: self.door_rotation(),
                                near: 0.04
                            },
                            action: WatcherAction::SetCollider(collider.map(|x| Box::new(x))),
                            ..Default::default()
                        };

                        watchers.replace(vec![collider_watcher]);
                    }
                }
            }
        }
    }

    pub fn door_occluder(&self) -> Option<Occluder>
    {
        (!self.open).then_some(Occluder::Door)
    }

    pub fn door_collider(&self) -> Option<Collider>
    {
        (!self.open).then(||
        {
            ColliderInfo{
                kind: ColliderType::Rectangle,
                layer: ColliderLayer::Door,
                ..Default::default()
            }.into()
        })
    }

    pub fn door_transform(&self) -> Transform
    {
        let offset_inside = 0.075;

        let rotation = self.door_rotation();

        let offset = -(TILE_SIZE / 2.0 + TILE_SIZE * offset_inside)
            + (self.width as f32 * TILE_SIZE) / 2.0;

        let mut position = self.position;
        position += rotate_point_z_3d(
            Vector3::new(offset, 0.0, 0.0),
            rotation
        );

        Transform{
            position,
            scale: Vector2::new(self.width as f32 + offset_inside * 2.0, 0.3).xyx() * TILE_SIZE,
            rotation,
            ..Default::default()
        }
    }

    pub fn texture(&self) -> String
    {
        format!(
            "furniture/{}_door{}.png",
            <&str>::from(self.material).to_lowercase(),
            self.width
        )
    }
}
