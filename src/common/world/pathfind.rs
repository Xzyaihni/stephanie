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
    values: Vec<TilePos>
}

impl WorldPath
{
    pub fn new(values: Vec<TilePos>) -> Self
    {
        Self{values}
    }

    pub fn move_along(&mut self, position: Vector3<f32>) -> Option<Vector3<f32>>
    {
        Some(Vector3::zeros())
    }

    pub fn debug_display(&self, entities: &ClientEntities)
    {
        self.values.iter().for_each(|pos|
        {
            entities.push(true, EntityInfo{
                transform: Some(Transform{
                    position: Vector3::from(pos.position()) + Vector3::repeat(TILE_SIZE * 0.5),
                    scale: Vector3::repeat(TILE_SIZE * 0.3),
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "circle.png".to_owned()
                    }.into()),
                    mix: Some(MixColor{keep_transparency: true, ..MixColor::color([0.0, 0.0, 1.0, 0.5])}),
                    above_world: true,
                    ..Default::default()
                }),
                watchers: Some(Watchers::simple_one_frame()),
                ..Default::default()
            });
        });

        self.values.iter().zip(self.values.iter().skip(1)).for_each(|(previous, current)|
        {
            if let Some(info) = line_info(
                Vector3::from(previous.position()) + Vector3::repeat(TILE_SIZE * 0.5),
                Vector3::from(current.position()) + Vector3::repeat(TILE_SIZE * 0.5),
                TILE_SIZE * 0.1,
                [0.2, 0.2, 1.0]
            )
            {
                entities.push(true, info);
            }
        });
    }
}
