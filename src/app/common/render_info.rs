use std::{
    fmt::{self, Debug},
    sync::Arc
};

use strum::AsRefStr;

use parking_lot::RwLock;

use serde::{Serialize, Deserialize};

use vulkano::{
    buffer::BufferContents,
    pipeline::graphics::viewport::Scissor as VulkanoScissor
};

use yanyaengine::{
    Object,
    ObjectInfo,
    TextObject,
    TextureId,
    DefaultModel,
    Transform,
    TransformContainer,
    TextInfo,
    object::Texture,
    game_object::*
};

pub use yanyaengine::object::model::Uvs;

use crate::{
    client::{RenderCreateInfo, VisibilityChecker},
    common::ServerToClient
};


#[repr(C)]
#[derive(BufferContents)]
pub struct OutlinedInfo
{
    other_color: [f32; 3],
    other_mix: f32,
    outlined: i32
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MixColor
{
    pub color: [f32; 3],
    pub amount: f32
}

impl OutlinedInfo
{
    pub fn new(
        other_color: Option<MixColor>,
        outline: bool
    ) -> Self
    {
        Self{
            other_color: other_color.map(|x| x.color).unwrap_or_default(),
            other_mix: other_color.map(|x| x.amount).unwrap_or_default(),
            outlined: outline as i32
        }
    }
}

#[derive(Debug)]
pub enum RenderComponent
{
    Full(RenderInfo),
    Object(RenderObject),
    Scissor(Scissor)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderObjectKind
{
    Texture{name: String},
    TextureId{id: TextureId},
    Text{text: String, font_size: u32}
}

impl RenderObjectKind
{
    pub fn into_client(
        self,
        uvs: Uvs,
        transform: Transform,
        create_info: &mut RenderCreateInfo
    ) -> Option<ClientRenderObject>
    {
        let assets = create_info.object_info.partial.assets.lock();

        match self
        {
            Self::TextureId{id} =>
            {
                let info = ObjectInfo{
                    model: assets.model(create_info.squares[&uvs]).clone(),
                    texture: assets.texture(id).clone(),
                    transform
                };

                let object = create_info.object_info.partial.object_factory.create(info);

                Some(ClientRenderObject{
                    kind: ClientObjectType::Normal(object),
                    outlined: false
                })
            },
            Self::Texture{name} =>
            {
                let id = assets.texture_id(&name);
                drop(assets);

                Self::TextureId{id}.into_client(uvs, transform, create_info)
            },
            Self::Text{ref text, font_size} =>
            {
                let object = create_info.object_info.partial.builder_wrapper.create_text(
                    TextInfo{
                        transform,
                        font_size,
                        text
                    },
                    create_info.location,
                    create_info.shader
                );

                if object.object.is_none()
                {
                    None
                } else
                {
                    Some(ClientRenderObject{
                        kind: ClientObjectType::Text(object),
                        outlined: false
                    })
                }
            }
        }
    }
}

impl RenderObject
{
    pub fn into_client(
        self,
        transform: Transform,
        create_info: &mut RenderCreateInfo
    ) -> Option<ClientRenderObject>
    {
        self.kind.into_client(self.flip, transform, create_info)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderObject
{
    pub kind: RenderObjectKind,
    pub flip: Uvs
}

impl From<RenderObjectKind> for RenderObject
{
    fn from(kind: RenderObjectKind) -> Self
    {
        Self{kind, flip: Uvs::default()}
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BoundingShape
{
    Circle
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scissor
{
    pub offset: [f32; 2],
    pub extent: [f32; 2]
}

impl Default for Scissor
{
    fn default() -> Self
    {
        Self{offset: [0.0, 0.0], extent: [1.0, 1.0]}
    }
}

impl Scissor
{
    pub fn into_global(self, size: [f32; 2]) -> VulkanoScissor
    {
        let [x, y] = size;

        let s = |value, s|
        {
            (value * s) as u32
        };

        VulkanoScissor{
            offset: [s(self.offset[0], x), s(self.offset[1], y)],
            extent: [s(self.extent[0], x), s(self.extent[1], y)]
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ZLevel
{
    BelowFeet = 0,
    Feet,
    Knee,
    Hips,
    Waist,
    Arms,
    Elbow,
    Shoulders,
    Head,
    Hair,
    Hat,
    UiLow,
    UiMiddle,
    UiHigh,
    UiHigher
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderInfo
{
    pub visible: bool,
    pub scissor: Option<Scissor>,
    pub object: Option<RenderObject>,
    pub shape: Option<BoundingShape>,
    pub mix: Option<MixColor>,
    pub z_level: ZLevel
}

impl Default for RenderInfo
{
    fn default() -> Self
    {
        Self{
            visible: true,
            scissor: None,
            object: None,
            shape: Some(BoundingShape::Circle),
            mix: None,
            z_level: ZLevel::Shoulders
        }
    }
}

#[derive(AsRefStr)]
pub enum ClientObjectType
{
    Normal(Object),
    Text(TextObject)
}

impl Debug for ClientObjectType
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "ClientObjectType::{}", self.as_ref())
    }
}

#[derive(Debug)]
pub struct ClientRenderObject
{
    kind: ClientObjectType,
    outlined: bool
}

impl ClientRenderObject
{
    pub fn set_transform(&mut self, transform: Transform)
    {
        match &mut self.kind
        {
            ClientObjectType::Normal(x) => x.set_transform(transform),
            ClientObjectType::Text(x) =>
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

    pub fn set_outlined(&mut self, outlined: bool)
    {
        self.outlined = outlined;
    }

    fn transform(&self) -> Option<&Transform>
    {
        match &self.kind
        {
            ClientObjectType::Normal(x) => Some(x.transform_ref()),
            ClientObjectType::Text(x) => x.transform()
        }
    }

    fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
        match &mut self.kind
        {
            ClientObjectType::Normal(x) => x.update_buffers(info),
            ClientObjectType::Text(x) => x.update_buffers(info)
        }
    }

    fn draw(&self, info: &mut DrawInfo, mix: Option<MixColor>)
    {
        info.push_constants(OutlinedInfo::new(mix, self.outlined));

        match &self.kind
        {
            ClientObjectType::Normal(x) => x.draw(info),
            ClientObjectType::Text(x) => x.draw(info)
        }
    }
}

#[derive(Debug)]
pub struct ClientRenderInfo
{
    pub visible: bool,
    pub scissor: Option<VulkanoScissor>,
    pub object: Option<ClientRenderObject>,
    pub shape: Option<BoundingShape>,
    pub mix: Option<MixColor>,
    pub z_level: ZLevel
}

impl ServerToClient<ClientRenderInfo> for RenderInfo
{
    fn server_to_client(
        self,
        transform: impl FnOnce() -> Transform,
        create_info: &mut RenderCreateInfo
    ) -> ClientRenderInfo
    {
        let object = self.object.and_then(|object|
        {
            object.into_client(transform(), create_info)
        });

        let scissor = self.scissor.map(|x|
        {
            x.into_global(create_info.object_info.partial.size)
        });

        ClientRenderInfo{
            visible: self.visible,
            scissor,
            object,
            shape: self.shape,
            mix: self.mix,
            z_level: self.z_level
        }
    }
}

impl ClientRenderInfo
{
    pub fn set_texture(&mut self, texture: Arc<RwLock<Texture>>)
    {
        if let Some(ClientRenderObject{
            kind: ClientObjectType::Normal(x),
            ..
        }) = self.object.as_mut()
        {
            x.set_texture(texture);
        }
    }

    pub fn set_inplace_texture(&mut self, texture: Texture)
    {
        if let Some(ClientRenderObject{
            kind: ClientObjectType::Normal(x),
            ..
        }) = self.object.as_mut()
        {
            x.set_inplace_texture(texture);
        }
    }

    pub fn set_outlined(&mut self, outlined: bool)
    {
        if let Some(object) = self.object.as_mut()
        {
            object.set_outlined(outlined);
        }
    }

    pub fn set_sprite(
        &mut self,
        create_info: &mut RenderCreateInfo,
        transform: Option<&Transform>,
        texture: TextureId
    )
    {
        let object_info = &mut create_info.object_info.partial;
        let assets = object_info.assets.lock();

        let texture = assets.texture(texture).clone();

        if let Some(ClientRenderObject{
            kind: ClientObjectType::Normal(x),
            ..
        }) = self.object.as_mut()
        {
            x.set_texture(texture);
        } else
        {
            let info = ObjectInfo{
                model: assets.model(assets.default_model(DefaultModel::Square)).clone(),
                texture,
                transform: transform.expect("renderable must have a transform").clone()
            };

            let object = ClientRenderObject{
                kind: ClientObjectType::Normal(
                    object_info.object_factory.create(info)
                ),
                outlined: false
            };

            self.object = Some(object);
        }
    }

    pub fn set_transform(&mut self, transform: Transform)
    {
        if let Some(x) = self.object.as_mut()
        {
            x.set_transform(transform);
        }
    }

    pub fn set_visibility(&mut self, visible: bool)
    {
        self.visible = visible;
    }

    fn visible(
        &self,
        visibility: &VisibilityChecker,
        transform: &Transform
    ) -> bool
    {
        if !self.visible
        {
            return false;
        }

        let shape = if let Some(x) = self.shape
        {
            x
        } else
        {
            return true;
        };

        visibility.visible(shape, transform)
    }

    pub fn update_buffers(
        &mut self,
        visibility: &VisibilityChecker,
        info: &mut UpdateBuffersInfo
    )
    {
        if !self.visible
        {
            return;
        }

        if let Some(transform) = self.object.as_ref().and_then(|x| x.transform())
        {
            if !self.visible(visibility, transform)
            {
                return;
            }
        }

        if let Some(object) = self.object.as_mut()
        {
            object.update_buffers(info);
        }
    }

    pub fn draw(
        &self,
        visibility: &VisibilityChecker,
        info: &mut DrawInfo
    )
    {
        if let Some(object) = self.object.as_ref()
        {
            if !self.visible
            {
                return;
            }

            if let Some(transform) = object.transform()
            {
                if !self.visible(visibility, transform)
                {
                    return;
                }
            }

            if let Some(scissor) = self.scissor
            {
                info.set_scissor(scissor);
            }

            object.draw(info, self.mix);

            if self.scissor.is_some()
            {
                info.reset_scissor();
            }
        }
    }
}
