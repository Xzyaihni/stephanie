use std::{
    fmt::Debug,
    sync::Arc
};

use strum::FromRepr;

use parking_lot::Mutex;

use serde::{Serialize, Deserialize};

use vulkano::buffer::BufferContents;

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

pub use yanyaengine::{TextCreateInfo, object::model::Uvs};

pub use vulkano::pipeline::graphics::viewport::Scissor as VulkanoScissor;

use crate::{
    client::VisibilityChecker,
    common::{colors::Lcha, ServerToClient}
};


#[repr(C)]
#[derive(BufferContents)]
pub struct OutlinedInfo
{
    other_color: [f32; 4],
    other_mix: f32,
    animation: f32,
    outlined: f32,
    keep_transparency: u32
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MixColorGeneric<T>
{
    pub color: T,
    pub amount: f32,
    pub keep_transparency: bool
}

impl<T> MixColorGeneric<T>
{
    pub fn color(color: T) -> Self
    {
        Self{color, amount: 1.0, keep_transparency: false}
    }
}

pub type MixColor = MixColorGeneric<[f32; 4]>;
pub type MixColorLch = MixColorGeneric<Lcha>;

impl From<MixColorLch> for MixColor
{
    fn from(color: MixColorLch) -> Self
    {
        Self{
            color: color.color.into(),
            amount: color.amount,
            keep_transparency: color.keep_transparency
        }
    }
}

struct RawMixColor
{
    other_color: [f32; 4],
    other_mix: f32,
    keep_transparency: u32
}

impl From<Option<MixColor>> for RawMixColor
{
    fn from(color: Option<MixColor>) -> Self
    {
        if let Some(color) = color
        {
            Self{
                other_color: color.color,
                other_mix: color.amount,
                keep_transparency: color.keep_transparency as u32
            }
        } else
        {
            Self{
                other_color: [0.0; 4],
                other_mix: 0.0,
                keep_transparency: 1
            }
        }
    }
}

impl OutlinedInfo
{
    pub fn new(
        other_color: Option<MixColor>,
        outlined: f32,
        animation: f32
    ) -> Self
    {
        let other_color = RawMixColor::from(other_color);

        Self{
            other_color: other_color.other_color,
            other_mix: other_color.other_mix,
            animation,
            outlined,
            keep_transparency: other_color.keep_transparency
        }
    }
}

#[repr(C)]
#[derive(BufferContents)]
pub struct UiOutlinedInfo
{
    other_color: [f32; 4],
    other_mix: f32,
    keep_transparency: u32
}

impl UiOutlinedInfo
{
    pub fn new(
        other_color: Option<MixColor>
    ) -> Self
    {
        let other_color = RawMixColor::from(other_color);

        Self{
            other_color: other_color.other_color,
            other_mix: other_color.other_mix,
            keep_transparency: other_color.keep_transparency
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
    Text{text: String, font_size: u32}
}

impl RenderObjectKind
{
    pub fn into_client(
        self,
        transform: Transform,
        create_info: &mut UpdateBuffersInfo
    ) -> Option<ClientRenderObject>
    {
        let assets = create_info.partial.assets.lock();

        match self
        {
            Self::TextureId{id} =>
            {
                let info = ObjectInfo{
                    model: assets.model(assets.default_model(DefaultModel::Square)).clone(),
                    texture: assets.texture(id).clone(),
                    transform
                };

                let object = create_info.partial.object_factory.create(info);

                Some(ClientRenderObject{
                    kind: ClientObjectType::Normal(object)
                })
            },
            Self::Texture{name} =>
            {
                let id = assets.texture_id(&name);
                drop(assets);

                Self::TextureId{id}.into_client(transform, create_info)
            },
            Self::Text{ref text, font_size} =>
            {
                let object = create_info.partial.builder_wrapper.create_text(
                    TextCreateInfo{
                        transform,
                        inner: TextInfo{
                            font_size,
                            text
                        }
                    }
                );

                if object.object.is_none()
                {
                    None
                } else
                {
                    Some(ClientRenderObject{
                        kind: ClientObjectType::Text(object)
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
        create_info: &mut UpdateBuffersInfo
    ) -> Option<ClientRenderObject>
    {
        self.kind.into_client(transform, create_info)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderObject
{
    pub kind: RenderObjectKind
}

impl From<RenderObjectKind> for RenderObject
{
    fn from(kind: RenderObjectKind) -> Self
    {
        Self{kind}
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, FromRepr, Serialize, Deserialize)]
pub enum ZLevel
{
    BelowFeet = 0,
    Feet,
    Knee,
    Hips,
    Waist,
    HandLow,
    Held,
    HandHigh,
    Elbow,
    Shoulders,
    Head,
    Hair,
    PlayerHead,
    PlayerHair,
    Hat,
    Door
}

impl ZLevel
{
    pub fn highest() -> Self
    {
        Self::Door
    }

    pub fn prev(self) -> Option<Self>
    {
        (self as usize).checked_sub(1).and_then(Self::from_repr)
    }

    pub fn next(self) -> Option<Self>
    {
        Self::from_repr(self as usize + 1)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderInfo
{
    pub visible: bool,
    pub shadow_visible: bool,
    pub scissor: Option<Scissor>,
    pub object: Option<RenderObject>,
    pub visibility_check: bool,
    pub above_world: bool,
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
            shadow_visible: false,
            scissor: None,
            object: None,
            visibility_check: true,
            above_world: false,
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

#[derive(Debug)]
pub enum ClientObjectType
{
    Normal(Object),
    Text(TextObject)
}

#[derive(Debug)]
pub struct ClientRenderObject
{
    kind: ClientObjectType
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
                if let Some(object) = x.object.as_mut()
                {
                    object.set_transform(transform);
                }
            }
        }
    }

    pub fn modify_transform(&mut self, f: impl FnOnce(&mut Transform))
    {
        if let Some(mut transform) = self.transform().cloned()
        {
            f(&mut transform);

            self.set_transform(transform);
        }
    }

    pub fn transform(&self) -> Option<&Transform>
    {
        match &self.kind
        {
            ClientObjectType::Normal(x) => Some(x.transform_ref()),
            ClientObjectType::Text(x) => x.transform()
        }
    }

    pub fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
        match &mut self.kind
        {
            ClientObjectType::Normal(x) => x.update_buffers(info),
            ClientObjectType::Text(x) => x.update_buffers(info)
        }
    }

    pub fn draw(&self, info: &mut DrawInfo)
    {
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
    pub shadow_visible: bool,
    pub scissor: Option<VulkanoScissor>,
    pub object: Option<ClientRenderObject>,
    pub visibility_check: bool,
    pub above_world: bool,
    pub mix: Option<MixColor>,
    pub aspect: Aspect,
    z_level: ZLevel
}

impl ServerToClient<ClientRenderInfo> for RenderInfo
{
    fn server_to_client(
        self,
        transform: impl FnOnce() -> Transform,
        create_info: &mut UpdateBuffersInfo
    ) -> ClientRenderInfo
    {
        let transform = transform();
        let object = self.object.and_then(|object|
        {
            object.into_client(transform.clone(), create_info)
        });

        let scissor = self.scissor.map(|x|
        {
            x.into_global(create_info.partial.size)
        });

        let mut this = ClientRenderInfo{
            visible: self.visible,
            shadow_visible: self.shadow_visible,
            scissor,
            object,
            visibility_check: self.visibility_check,
            above_world: self.above_world,
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
    pub fn set_texture(&mut self, texture: Arc<Mutex<Texture>>)
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

    pub fn as_text(&self) -> Option<&TextObject>
    {
        if let Some(ClientRenderObject{
            kind: ClientObjectType::Text(text),
            ..
        }) = self.object.as_ref()
        {
            Some(text)
        } else
        {
            None
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

    pub fn texture(&self) -> Option<&Arc<Mutex<Texture>>>
    {
        match &self.object.as_ref()?.kind
        {
            ClientObjectType::Normal(x) => Some(x.texture()),
            ClientObjectType::Text(x) => x.texture()
        }
    }

    pub fn set_sprite(
        &mut self,
        create_info: &mut UpdateBuffersInfo,
        transform: Option<&Transform>,
        texture: TextureId
    )
    {
        let object_info = &mut create_info.partial;
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
                )
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
                    let aspect = texture.lock().aspect_min();

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
            true
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

    pub fn draw<T: BufferContents>(
        &self,
        info: &mut DrawInfo,
        shader_value: T
    )
    {
        if let Some(object) = self.object.as_ref()
        {
            if !self.visible
            {
                return;
            }

            if let Some(scissor) = self.scissor
            {
                info.set_scissor(scissor);
            }

            info.push_constants(shader_value);

            object.draw(info);

            if self.scissor.is_some()
            {
                info.reset_scissor();
            }
        }
    }
}
