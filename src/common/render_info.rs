use std::{
    f32,
    fmt::Debug,
    sync::Arc,
    borrow::Cow
};

use strum::FromRepr;

use parking_lot::{Mutex, RwLock};

use nalgebra::Vector2;

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
    object::{Model, Texture},
    game_object::*
};

pub use yanyaengine::{TextCreateInfo, object::model::Uvs};

pub use vulkano::pipeline::graphics::viewport::Scissor as VulkanoScissor;

use crate::{
    client::{SlicedTexture, VisibilityChecker},
    common::{
        lerp,
        with_z,
        rotate_point,
        colors::{srgb_to_linear, Lcha},
        ServerToClient,
        world::{TileRotation, PosDirection, DirectionsGroup}
    }
};


pub trait RenderInfoTrait
{
    fn is_visible(&self) -> bool;
    fn set_visible(&mut self, value: bool);
}

#[repr(C)]
#[derive(BufferContents)]
pub struct OutlinedInfo
{
    other_color: [f32; 4],
    other_mix: f32,
    animation: f32,
    outlined: f32,
    flags: u32
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MixColorGeneric<T>
{
    pub color: T,
    pub amount: f32,
    pub only_alpha: bool,
    pub keep_transparency: bool
}

impl<T: Default> Default for MixColorGeneric<T>
{
    fn default() -> Self
    {
        Self{
            color: T::default(),
            amount: 0.0,
            only_alpha: false,
            keep_transparency: true
        }
    }
}

impl<T> MixColorGeneric<T>
{
    pub fn color(color: T) -> Self
    {
        Self{color, amount: 1.0, only_alpha: false, keep_transparency: true}
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
            only_alpha: color.only_alpha,
            keep_transparency: color.keep_transparency
        }
    }
}

struct RawMixColor
{
    other_color: [f32; 4],
    other_mix: f32,
    flags: u32
}

impl From<Option<MixColor>> for RawMixColor
{
    fn from(color: Option<MixColor>) -> Self
    {
        fn flags(only_alpha: bool, keep_transparency: bool) -> u32
        {
            ((only_alpha as u32) << 1) | keep_transparency as u32
        }

        if let Some(color) = color
        {
            let [r, g, b, a] = color.color;
            let [new_r, new_g, new_b] = srgb_to_linear([r, g, b]);

            Self{
                other_color: [new_r, new_g, new_b, a],
                other_mix: color.amount,
                flags: flags(color.only_alpha, color.keep_transparency)
            }
        } else
        {
            Self{
                other_color: [0.0; 4],
                other_mix: 0.0,
                flags: flags(false, true)
            }
        }
    }
}

impl OutlinedInfo
{
    pub fn new(
        other_color: Option<MixColor>,
        outlined: bool,
        animation: f32
    ) -> Self
    {
        let other_color = RawMixColor::from(other_color);

        Self{
            other_color: other_color.other_color,
            other_mix: other_color.other_mix,
            animation,
            outlined: if outlined { 1.0 } else { 0.0 },
            flags: other_color.flags
        }
    }
}

#[repr(C)]
#[derive(BufferContents)]
pub struct UiOutlinedInfo
{
    other_color: [f32; 4],
    other_mix: f32,
    flags: u32
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
            flags: other_color.flags
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiElementFill
{
    pub full: Lcha,
    pub empty: Lcha,
    pub amount: f32,
    pub horizontal: bool
}

#[derive(Debug, BufferContents)]
#[repr(C)]
pub struct FillInfo
{
    other_color: [f32; 4],
    full_color: [f32; 4],
    empty_color: [f32; 4],
    other_mix: f32,
    flags: u32,
    amount: f32
}

impl UiElementFill
{
    pub fn into_info(&self, color: Option<MixColor>) -> FillInfo
    {
        fn color_convert([r, g, b, a]: [f32; 4]) -> [f32; 4]
        {
            let [r, g, b] = srgb_to_linear([r, g, b]);

            [r, g, b, a]
        }

        let color = RawMixColor::from(color);

        debug_assert!(color.flags & ((1 << 2) - 1) == color.flags, "uh oh colors shouldnt have more than 2 flags");

        FillInfo{
            other_color: color.other_color,
            full_color: color_convert(self.full.into()),
            empty_color: color_convert(self.empty.into()),
            other_mix: color.other_mix,
            flags: color.flags | ((self.horizontal as u32) << 2),
            amount: self.amount
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

fn sprite_rotation(rotation: f32) -> PosDirection
{
    PosDirection::from(TileRotation::from_angle(rotation).rotate_counterclockwise()).flip_x()
}

pub fn rotating_info<T: Clone>(
    transform: Transform,
    offset: Option<f32>,
    textures: &DirectionsGroup<T>
) -> (T, Transform)
{
    let current_direction = sprite_rotation(transform.rotation);
    let closest = textures[current_direction].clone();

    let visual_rotation = {
        let x = (transform.rotation + f32::consts::FRAC_PI_4) % f32::consts::FRAC_PI_2;

        let x = if x < 0.0 { x + f32::consts::FRAC_PI_2 } else { x };

        x - f32::consts::FRAC_PI_4
    };

    let scale = if offset.is_none() && current_direction.is_horizontal()
    {
        transform.scale.yxz()
    } else
    {
        transform.scale
    };

    let position = if let Some(x) = offset
    {
        let s = transform.scale.xy();

        let wide = s.x > s.y;

        let l = |a| lerp(-0.5 * s.y + 0.5 * s.x, 0.5 * s.y - 0.5 * s.x, a);

        let position = if wide { Vector2::new(l(1.0 - x), 0.0) } else { Vector2::new(0.0, l(x)) };

        let full_rotation = TileRotation::from_angle(transform.rotation).to_angle();

        transform.position + with_z(rotate_point(position, transform.rotation - full_rotation), 0.0)
    } else
    {
        transform.position
    };

    let object_transform = Transform{
        position,
        rotation: visual_rotation,
        scale,
        ..transform
    };

    (closest, object_transform)
}

fn rotating_scale(transform: Transform, texture_scale: Vector2<f32>) -> Transform
{
    Transform{scale: with_z(transform.scale.xy().component_mul(&texture_scale), transform.scale.z), ..transform}
}

fn sliced_model(
    width_unscaled: f32,
    height_unscaled: f32,
    scale: Vector2<f32>
) -> Model
{
    let w = (width_unscaled / scale.x).min(0.5);
    let h = (height_unscaled / scale.y).min(0.5);

    let sx = -0.5;
    let sy = -0.5;

    let ex = 0.5;
    let ey = 0.5;

    let vertices = vec![
        [sx, sy, 0.0],
        [sx, sy + h, 0.0],
        [sx + w, sy, 0.0],
        [sx + w, sy + h, 0.0],

        [ex - w, sy, 0.0],
        [ex - w, sy + h, 0.0],
        [ex, sy, 0.0],
        [ex, sy + h, 0.0],

        [sx, ey - h, 0.0],
        [sx, ey, 0.0],
        [sx + w, ey - h, 0.0],
        [sx + w, ey, 0.0],

        [ex - w, ey - h, 0.0],
        [ex - w, ey, 0.0],
        [ex, ey - h, 0.0],
        [ex, ey, 0.0]
    ];

    let indices = vec![
        0, 1, 2, // bottom left
        2, 1, 3,

        2, 3, 4, // bottom
        4, 3, 5,

        4, 5, 6, // bottom right
        6, 5, 7,

        3, 1, 8, // left
        3, 8, 10,

        8, 9, 10, // top left
        10, 9, 11,

        12, 10, 11, // top
        12, 11, 13,

        12, 13, 14, // top right
        14, 13, 15,

        7, 5, 12, // right
        7, 12, 14,

        5, 3, 10, // middle
        5, 10, 12
    ];

    let uvs = vec![
        [0.0, 0.0],
        [0.0, height_unscaled],
        [width_unscaled, 0.0],
        [width_unscaled, height_unscaled],

        [1.0 - width_unscaled, 0.0],
        [1.0 - width_unscaled, height_unscaled],
        [1.0, 0.0],
        [1.0, height_unscaled],

        [0.0, 1.0 - height_unscaled],
        [0.0, 1.0],
        [width_unscaled, 1.0 - height_unscaled],
        [width_unscaled, 1.0],

        [1.0 - width_unscaled, 1.0 - height_unscaled],
        [1.0 - width_unscaled, 1.0],
        [1.0, 1.0 - height_unscaled],
        [1.0, 1.0]
    ];

    Model{
        vertices,
        indices,
        uvs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderObjectKind
{
    Texture{name: Cow<'static, str>},
    TextureId{id: TextureId},
    TextureRotating{ids: DirectionsGroup<TextureId>, offset: Option<f32>},
    TextureSliced{texture: SlicedTexture, normal_scale: Vector2<f32>},
    Text(TextInfo<'static>)
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
            Self::TextureSliced{texture: sliced, normal_scale} =>
            {
                let texture = assets.texture(sliced.id).clone();

                let model = sliced_model(sliced.width, sliced.height, transform.scale.xy().component_div(&normal_scale));

                let object = create_info.partial.object_factory.create(ObjectInfo{
                    model: Arc::new(RwLock::new(model)),
                    texture,
                    transform
                });

                Some(ClientRenderObject{
                    kind: ClientObjectType::NormalSliced{object, width: sliced.width, height: sliced.height, normal_scale}
                })
            },
            Self::TextureRotating{ids, offset} =>
            {
                let textures = ids.map(|_, id| assets.texture(id).clone());

                let up_size = textures.up.lock().size();
                let textures = textures.map(|direction, texture|
                {
                    let size = texture.lock().size();

                    let size = if offset.is_none() && direction.is_horizontal()
                    {
                        size.yx()
                    } else
                    {
                        size
                    };

                    (up_size.component_div(&size), texture)
                });

                let current_direction = sprite_rotation(transform.rotation);

                let (scale_factor, texture) = &textures[current_direction];
                let object = create_info.partial.object_factory.create(ObjectInfo{
                    model: assets.model(assets.default_model(DefaultModel::Square)).clone(),
                    texture: texture.clone(),
                    transform: rotating_scale(transform.clone(), *scale_factor)
                });

                Some(ClientRenderObject{
                    kind: ClientObjectType::NormalRotating{object, offset, textures}
                })
            },
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
            Self::Text(inner) =>
            {
                let object = create_info.partial.builder_wrapper.create_text(
                    TextCreateInfo{
                        transform,
                        inner
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
    pub fn into_global(mut self, size: [f32; 2]) -> VulkanoScissor
    {
        let [x, y] = size;

        let s = |value, s|
        {
            (value * s) as u32
        };

        (0..2).for_each(|i|
        {
            let offset = &mut self.offset[i];
            if *offset < 0.0
            {
                self.extent[i] += *offset;

                *offset = 0.0;
            }
        });

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
    pub outlined: bool,
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
            outlined: false,
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

impl RenderInfoTrait for RenderInfo
{
    fn is_visible(&self) -> bool
    {
        self.visible
    }

    fn set_visible(&mut self, value: bool)
    {
        self.visible = value;
    }
}

#[derive(Debug)]
pub enum ClientObjectType
{
    Normal(Object),
    NormalRotating{object: Object, offset: Option<f32>, textures: DirectionsGroup<(Vector2<f32>, Arc<Mutex<Texture>>)>},
    NormalSliced{object: Object, width: f32, height: f32, normal_scale: Vector2<f32>},
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
            ClientObjectType::NormalRotating{object, offset, textures} =>
            {
                let ((scale, closest), object_transform) = rotating_info(transform, *offset, textures);

                object.set_texture(closest);
                object.set_transform(rotating_scale(object_transform, scale));
            },
            ClientObjectType::NormalSliced{object, width, height, normal_scale} =>
            {
                object.set_inplace_model_same_sized(sliced_model(*width, *height, transform.scale.xy().component_div(normal_scale)));
                object.set_transform(transform);
            },
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
            ClientObjectType::NormalRotating{object, ..} => Some(object.transform_ref()),
            ClientObjectType::NormalSliced{object, ..} => Some(object.transform_ref()),
            ClientObjectType::Text(x) => x.transform()
        }
    }

    pub fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
        match &mut self.kind
        {
            ClientObjectType::Normal(x) => x.update_buffers(info),
            ClientObjectType::NormalRotating{object, ..} => object.update_buffers(info),
            ClientObjectType::NormalSliced{object, ..} => object.update_buffers(info),
            ClientObjectType::Text(x) => x.update_buffers(info)
        }
    }

    pub fn draw(&self, info: &mut DrawInfo)
    {
        match &self.kind
        {
            ClientObjectType::Normal(x) => x.draw(info),
            ClientObjectType::NormalRotating{object, ..} => object.draw(info),
            ClientObjectType::NormalSliced{object, ..} => object.draw(info),
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
    pub outlined: bool,
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
            outlined: self.outlined,
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
            ClientObjectType::Text(x) => x.texture(),
            _ => None
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

    pub fn visible_broad(&self) -> bool
    {
        self.visible
    }

    pub fn visible_narrow(
        &self,
        visibility: &VisibilityChecker,
        transform: &Transform
    ) -> bool
    {
        if self.visibility_check
        {
            visibility.visible_sphere(transform)
        } else
        {
            true
        }
    }

    pub fn visible_with(
        &self,
        visibility: &VisibilityChecker,
        transform: &Transform
    ) -> bool
    {
        self.visible_broad() && self.visible_narrow(visibility, transform)
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

impl RenderInfoTrait for ClientRenderInfo
{
    fn is_visible(&self) -> bool
    {
        self.visible
    }

    fn set_visible(&mut self, value: bool)
    {
        self.visible = value;
    }
}
