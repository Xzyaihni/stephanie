use std::iter;

use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    line_info,
    watcher::*,
    render_info::*,
    AnyEntities,
    EntityInfo,
    world::{
        TILE_SIZE,
        ClientEntities,
        TilePos
    }
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPath
{
    values: Vec<TilePos>,
    target: Vector3<f32>
}

impl WorldPath
{
    pub fn new(
        values: Vec<TilePos>,
        target: Vector3<f32>
    ) -> Self
    {
        Self{values, target}
    }

    pub fn target_tile(&self) -> Option<&TilePos>
    {
        self.values.first()
    }

    pub fn remove_current_target(&mut self)
    {
        self.values.pop();
    }

    pub fn move_along(
        &mut self,
        near: f32,
        position: Vector3<f32>
    ) -> Option<Vector3<f32>>
    {
        if self.values.is_empty()
        {
            let distance = self.target - position;

            if distance.magnitude() < near
            {
                return None;
            }

            return Some(distance);
        }

        let target_position: Vector3<f32> = self.values.last().unwrap().center_position().into();

        let distance = target_position - position;

        if distance.magnitude() < near
        {
            self.remove_current_target();
            return self.move_along(near, position)
        }

        Some(distance)
    }

    pub fn debug_display(&self, entities: &ClientEntities)
    {
        let amount = self.values.len();
        iter::once((amount == 0, self.target)).chain(self.values.iter().enumerate().map(|(index, pos)|
        {
            ((index + 1) == amount, Vector3::from(pos.center_position()))
        })).for_each(|(is_selected, position)|
        {
            let color = if is_selected
            {
                [1.0, 0.0, 0.0, 0.5]
            } else
            {
                [0.0, 0.0, 1.0, 0.5]
            };

            entities.push(true, EntityInfo{
                transform: Some(Transform{
                    position,
                    scale: Vector3::repeat(TILE_SIZE * 0.3),
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "circle.png".to_owned()
                    }.into()),
                    mix: Some(MixColor{keep_transparency: true, ..MixColor::color(color)}),
                    above_world: true,
                    ..Default::default()
                }),
                watchers: Some(Watchers::simple_one_frame()),
                ..Default::default()
            });
        });

        iter::once(self.target).chain(self.values.iter().map(|x| Vector3::from(x.center_position())))
            .zip(self.values.iter().map(|x| Vector3::from(x.center_position())))
            .for_each(|(previous, current)|
            {
                if let Some(info) = line_info(previous, current, TILE_SIZE * 0.1, [0.2, 0.2, 1.0])
                {
                    entities.push(true, info);
                }
            });
    }
}
