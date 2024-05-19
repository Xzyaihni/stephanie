use serde::{Serialize, Deserialize};

use yanyaengine::{
    Object,
    ObjectInfo,
    TextObject,
    TextureId,
    DefaultModel,
    Transform,
    TransformContainer,
    TextInfo,
    game_object::*
};

use crate::common::ServerToClient;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderObject
{
    Texture{name: String},
    Text{text: String, font_size: u32}
}

impl RenderObject
{
    pub fn into_client(
        self,
        transform: Transform,
        create_info: &mut ObjectCreateInfo
    ) -> Option<ClientRenderObject>
    {
        let assets = create_info.partial.assets.lock();

        match self
        {
            Self::Texture{name} =>
            {
                let info = ObjectInfo{
                    model: assets.model(assets.default_model(DefaultModel::Square)).clone(),
                    texture: assets.texture_by_name(&name).clone(),
                    transform
                };

                Some(ClientRenderObject::Normal(create_info.partial.object_factory.create(info)))
            },
            Self::Text{ref text, font_size} =>
            {
                let object = create_info.partial.builder_wrapper.create_text(TextInfo{
                    transform,
                    font_size,
                    text
                });

                if object.object.is_none()
                {
                    None
                } else
                {
                    Some(ClientRenderObject::Text(object))
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderInfo
{
    pub object: Option<RenderObject>,
    pub z_level: i32
}

pub enum ClientRenderObject
{
    Normal(Object),
    Text(TextObject)
}

impl ClientRenderObject
{
    pub fn set_transform(&mut self, transform: Transform)
    {
        match self
        {
            Self::Normal(x) => x.set_transform(transform),
            Self::Text(x) =>
            {
                let mut scale_changed = false;
                if let Some(object) = x.object.as_mut()
                {
                    scale_changed = *object.scale() != transform.scale;
                    object.set_transform(transform);
                }

                if scale_changed
                {
                    x.update_scale();
                }
            }
        }
    }
}

impl GameObject for ClientRenderObject
{
    fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
        match self
        {
            Self::Normal(x) => x.update_buffers(info),
            Self::Text(x) => x.update_buffers(info)
        }
    }

    fn draw(&self, info: &mut DrawInfo)
    {
        match self
        {
            Self::Normal(x) => x.draw(info),
            Self::Text(x) => x.draw(info)
        }
    }
}

pub struct ClientRenderInfo
{
    pub object: Option<ClientRenderObject>,
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
        let object = self.object.and_then(|object|
        {
            object.into_client(transform.expect("renderable must have a transform"), create_info)
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
            match object
            {
                ClientRenderObject::Normal(x) =>
                {
                    x.set_texture(texture);
                },
                _ => ()
            }
        } else
        {
            let info = ObjectInfo{
                model: assets.model(assets.default_model(DefaultModel::Square)).clone(),
                texture,
                transform: transform.expect("renderable must have a transform").clone()
            };

            let object = ClientRenderObject::Normal(
                create_info.partial.object_factory.create(info)
            );

            self.object = Some(object);
        }
    }
}
