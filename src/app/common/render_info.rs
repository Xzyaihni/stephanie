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

pub use yanyaengine::{FontStyle, TextAlign, HorizontalAlign, VerticalAlign, object::model::Uvs};

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
    animation: f32,
    outlined: f32
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
        outlined: f32,
        animation: f32
    ) -> Self
    {
        Self{
            other_color: other_color.map(|x| x.color).unwrap_or_default(),
            other_mix: other_color.map(|x| x.amount).unwrap_or_default(),
            animation,
            outlined
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aspect
{
    KeepMax,
    Fill
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderObjectKind
{
    Texture{name: String},
    TextureId{id: TextureId},
    Text{text: String, font_size: u32, font: FontStyle, align: TextAlign}
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
                    outlined: None
                })
            },
            Self::Texture{name} =>
            {
                let id = assets.texture_id(&name);
                drop(assets);

                Self::TextureId{id}.into_client(uvs, transform, create_info)
            },
            Self::Text{ref text, font_size, font, align} =>
            {
                let object = create_info.object_info.partial.builder_wrapper.create_text(
                    TextInfo{
                        transform,
                        font_size,
                        font,
                        align,
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
                        outlined: None
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
    Door,
    UiLow,
    UiMiddle,
    UiHigh,
    UiHigher,
    UiPopupLow,
    UiPopupMiddle,
    UiPopupHigh
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderInfo
{
    pub visible: bool,
    pub scissor: Option<Scissor>,
    pub object: Option<RenderObject>,
    pub visibility_check: bool,
    pub mix: Option<MixColor>,
    pub aspect: Aspect,
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
            visibility_check: true,
            mix: None,
            aspect: Aspect::Fill,
            z_level: ZLevel::Shoulders
        }
    }
}

impl RenderInfo
{
    pub fn z_level(&self) -> ZLevel
    {
        self.z_level
    }

    pub fn set_z_level(&mut self, z_level: ZLevel)
    {
        self.z_level = z_level;
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
    outlined: Option<f32>
}

impl ClientRenderObject
{
    fn set_transform(&mut self, transform: Transform)
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

    pub fn set_outlined(&mut self, outlined: Option<f32>)
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

    fn draw(&self, info: &mut DrawInfo, mix: Option<MixColor>, animation: f32)
    {
        let outline = OutlinedInfo::new(
            mix,
            self.outlined.unwrap_or(0.0),
            animation
        );

        info.push_constants(outline);

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
    pub visibility_check: bool,
    pub mix: Option<MixColor>,
    pub aspect: Aspect,
    z_level: ZLevel
}

impl ServerToClient<ClientRenderInfo> for RenderInfo
{
    fn server_to_client(
        self,
        transform: impl FnOnce() -> Transform,
        create_info: &mut RenderCreateInfo
    ) -> ClientRenderInfo
    {
        let transform = transform();
        let object = self.object.and_then(|object|
        {
            object.into_client(transform.clone(), create_info)
        });

        let scissor = self.scissor.map(|x|
        {
            x.into_global(create_info.object_info.partial.size)
        });

        let mut this = ClientRenderInfo{
            visible: self.visible,
            scissor,
            object,
            visibility_check: self.visibility_check,
            mix: self.mix,
            aspect: self.aspect,
            z_level: self.z_level
        };

        let transform = this.transform_with_aspect(transform);
        this.set_transform(transform);

        this
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

    pub fn z_level(&self) -> ZLevel
    {
        self.z_level
    }

    pub fn set_z_level(&mut self, z_level: ZLevel)
    {
        self.z_level = z_level;
    }

    pub fn texture(&self) -> Option<&Arc<RwLock<Texture>>>
    {
        match &self.object.as_ref()?.kind
        {
            ClientObjectType::Normal(x) => Some(x.texture()),
            ClientObjectType::Text(x) => x.texture()
        }
    }

    pub fn set_outlined(&mut self, outlined: Option<f32>)
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
            let transform = transform.expect("renderable must have a transform").clone();
            let transform = self.transform_with_aspect(transform);

            let info = ObjectInfo{
                model: assets.model(assets.default_model(DefaultModel::Square)).clone(),
                texture,
                transform
            };

            let object = ClientRenderObject{
                kind: ClientObjectType::Normal(
                    object_info.object_factory.create(info)
                ),
                outlined: None
            };

            self.object = Some(object);
        }
    }

    pub fn set_transform(&mut self, transform: Transform)
    {
        let transform = self.transform_with_aspect(transform);
        if let Some(x) = self.object.as_mut()
        {
            x.set_transform(transform);
        }
    }

    pub fn set_visibility(&mut self, visible: bool)
    {
        self.visible = visible;
    }

    fn transform_with_aspect(&self, mut transform: Transform) -> Transform
    {
        match self.aspect
        {
            Aspect::Fill => transform,
            Aspect::KeepMax =>
            {
                if let Some(texture) = self.texture()
                {
                    let aspect = texture.read().aspect_min();

                    let scale = if aspect.y > aspect.x
                    {
                        transform.scale.yy()
                    } else
                    {
                        transform.scale.xx()
                    };

                    transform.scale = scale.component_mul(&aspect).xyx();
                }

                transform
            }
        }
    }

    pub fn visible(
        &self,
        visibility: &VisibilityChecker
    ) -> bool
    {
        self.object.as_ref().and_then(|x| x.transform()).map(|transform|
        {
            self.visible_with(visibility, transform)
        }).unwrap_or(false)
    }

    pub fn visible_with(
        &self,
        visibility: &VisibilityChecker,
        transform: &Transform
    ) -> bool
    {
        if !self.visible
        {
            return false;
        }

        if self.visibility_check
        {
            visibility.visible_sphere(transform)
        } else
        {
            return true;
        }
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        if !self.visible
        {
            return;
        }

        if let Some(object) = self.object.as_mut()
        {
            object.update_buffers(info);
        }
    }

    pub fn draw(
        &self,
        visibility: &VisibilityChecker,
        info: &mut DrawInfo,
        animation: f32
    )
    {
        if let Some(object) = self.object.as_ref()
        {
            if !self.visible
            {
                return;
            }

            if !self.visible(visibility)
            {
                return;
            }

            if let Some(scissor) = self.scissor
            {
                info.set_scissor(scissor);
            }

            object.draw(info, self.mix, animation);

            if self.scissor.is_some()
            {
                info.reset_scissor();
            }
        }
    }
}
