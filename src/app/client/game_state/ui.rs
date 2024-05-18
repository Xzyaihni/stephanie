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

        let mut z_level = 100;

        let mut add_ui = |parent, position, scale, ui_element|
        {
            z_level += 1;

            entities.push(EntityInfo{
                transform: Some(Default::default()),
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale,
                        position,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                ui_element: Some(ui_element),
                render: Some(RenderInfo{
                    texture: Some("ui/background.png".to_owned()),
                    z_level
                }.server_to_client(Some(Default::default()), object_info)),
                parent: Some(Parent::new(parent)),
                ..Default::default()
            })
        };

        let inventory = add_ui(
            anchor,
            Vector3::zeros(),
            Vector3::new(0.4, 0.4, 1.0),
            UiElement{
                kind: UiElementType::Panel
            }
        );

        let panel_size = 0.2;

        let top_panel = add_ui(
            inventory,
            Vector3::new(0.0, -(1.0 / 2.0 - panel_size / 2.0), 0.0),
            Vector3::new(1.0, panel_size, 1.0),
            UiElement{
                kind: UiElementType::Panel
            }
        );

        

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
