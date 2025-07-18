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
        ClientEntities
    }
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPath
{
    values: Vec<Vector3<f32>>
}

impl WorldPath
{
    pub fn new(values: Vec<Vector3<f32>>) -> Self
    {
        Self{values}
    }

    pub fn target(&self) -> Option<&Vector3<f32>>
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
            return None;
        }

        let target_position = self.values.last().unwrap();

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
        self.values.iter().copied().enumerate().for_each(|(index, position)|
        {
            let is_selected = (index + 1) == amount;

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

        self.values.iter().zip(self.values.iter().skip(1)).for_each(|(previous, current)|
        {
            if let Some(info) = line_info(*previous, *current, TILE_SIZE * 0.1, [0.2, 0.2, 1.0])
            {
                entities.push(true, info);
            }
        });
    }
}
