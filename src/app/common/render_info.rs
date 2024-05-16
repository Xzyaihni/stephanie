use serde::{Serialize, Deserialize};

use yanyaengine::{
    Object,
    ObjectInfo,
    TextureId,
    DefaultModel,
    Transform,
    game_object::*
};

use crate::common::ServerToClient;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderInfo
{
    pub texture: Option<String>,
    pub z_level: i32
}

pub struct ClientRenderInfo
{
    pub object: Option<Object>,
    pub z_level: i32
}

impl ServerToClient<ClientRenderInfo> for RenderInfo
{
    fn server_to_client(
        self,
        transform: Option<Transform>,
        create_info: &mut ObjectCreateInfo
    ) -> ClientRenderInfo
    {
        let object = self.texture.map(|texture|
        {
            let assets = create_info.partial.assets.lock();

            let info = ObjectInfo{
                model: assets.model(assets.default_model(DefaultModel::Square)).clone(),
                texture: assets.texture_by_name(&texture).clone(),
                transform: transform.expect("renderable must have a transform")
            };

            create_info.partial.object_factory.create(info)
        });

        ClientRenderInfo{object, z_level: self.z_level}
    }
}

impl ClientRenderInfo
{
    pub fn set_sprite(
        &mut self,
        create_info: &mut ObjectCreateInfo,
        transform: Option<&Transform>,
        texture: TextureId
    )
    {
        let assets = create_info.partial.assets.lock();

        let texture = assets.texture(texture).clone();

        if let Some(object) = self.object.as_mut()
        {
            object.set_texture(texture);
        } else
        {
            let info = ObjectInfo{
                model: assets.model(assets.default_model(DefaultModel::Square)).clone(),
                texture,
                transform: transform.expect("renderable must have a transform").clone()
            };

            self.object = Some(create_info.partial.object_factory.create(info));
        }
    }
}
