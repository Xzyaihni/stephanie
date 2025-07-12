use std::f32;

use strum::{EnumString, IntoStaticStr};

use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    rotate_point_z_3d,
    Entity,
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

    pub fn set_open(&mut self, entities: &ClientEntities, entity: Entity, state: bool)
    {
        if self.open != state
        {
            self.open = state;

            if let Some(parent) = entities.parent(entity)
            {
                if let Some(mut lazy) = entities.lazy_transform_mut(parent.entity())
                {
                    lazy.set_origin_rotation(if self.open { -f32::consts::FRAC_PI_2 } else { 0.0 });
                }
            }
        }
    }

    pub fn door_transform(&self) -> Transform
    {
        let offset_inside = 0.075;

        let rotation = self.rotation.to_angle() + f32::consts::PI;

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
