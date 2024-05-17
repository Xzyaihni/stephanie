use nalgebra::{Vector2, Vector3};

use yanyaengine::{Transform, game_object::*};

use crate::{
    client::ui_element::*,
    common::{
        Parent,
        Entity,
        ServerToClient,
        EntityInfo,
        RenderInfo,
        lazy_transform::*,
        entity::ClientEntities
    }
};


pub struct Ui
{
    anchor: Entity
}

impl Ui
{
    pub fn new(
        object_info: &mut ObjectCreateInfo,
        entities: &mut ClientEntities,
        _aspect: f32
    ) -> Self
    {
        let anchor = entities.push(EntityInfo{
            transform: Some(Default::default()),
            lazy_transform: Some(LazyTransformInfo{
                connection: Connection::Limit{limit: 1.0},
                ..Default::default()
            }.into()),
            ..Default::default()
        });

        entities.push(EntityInfo{
            transform: Some(Default::default()),
            lazy_transform: Some(LazyTransformInfo{
                transform: Transform{
                    scale: Vector3::new(0.4, 0.4, 1.0),
                    ..Default::default()
                },
                ..Default::default()
            }.into()),
            ui_element: Some(UiElement{
                kind: UiElementType::Panel
            }),
            render: Some(RenderInfo{
                texture: Some("ui/background.png".to_owned()),
                z_level: 100
            }.server_to_client(Some(Default::default()), object_info)),
            parent: Some(Parent::new(anchor)),
            ..Default::default()
        });

        Self{
            anchor
        }
    }

    pub fn update(
        &mut self,
        entities: &mut ClientEntities,
        player_transform: Option<Transform>,
        camera_size: Vector2<f32>
    )
    {
        let ui_transform = entities.lazy_transform_mut(self.anchor)
            .unwrap();

        let min_size = camera_size.x.min(camera_size.y);
        ui_transform.set_connection_limit(min_size * 0.3);

        let ui_target = ui_transform.target();

        let ui_scale = &mut ui_target.scale;

        ui_scale.x = camera_size.x;
        ui_scale.y = camera_size.y;

        if let Some(player_transform) = player_transform
        {
            ui_target.position = player_transform.position;
        }

        entities.update_ui(camera_size);
    }
}
