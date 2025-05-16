use std::{convert, fmt::Debug};

use nalgebra::Vector2;

use yanyaengine::TextureId;

use crate::common::render_info::*;

pub use crate::common::lazy_transform::{Scaling, Connection};


// i wanted to do this with FlatChunksContainer but i dont like how i made that one
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiElementShapeMask
{
    size: Vector2<usize>,
    values: Box<[bool]>
}

impl UiElementShapeMask
{
    pub fn new_empty(size: Vector2<usize>) -> Self
    {
        Self{size, values: vec![false; size.product()].into_boxed_slice()}
    }

    pub fn is_inside(&self, pos: Vector2<f32>) -> bool
    {
        let point = (pos.component_mul(&self.size.cast())).map(|x| x as i32);

        let size = self.size.map(|x| x as i32);
        if !((0..size.x).contains(&point.x) && (0..size.y).contains(&point.y))
        {
            return false;
        }

        let clamped = point.map(|x| x as usize);

        self.get(clamped).unwrap_or(false)
    }

    fn to_index(&self, pos: Vector2<usize>) -> Option<usize>
    {
        ((0..self.size.x).contains(&pos.x) && (0..self.size.y).contains(&pos.y)).then(||
        {
            pos.y * self.size.x + pos.x
        })
    }

    pub fn get(&self, pos: Vector2<usize>) -> Option<bool>
    {
        self.to_index(pos).map(|index| self.values[index])
    }

    pub fn get_mut(&mut self, pos: Vector2<usize>) -> Option<&mut bool>
    {
        self.to_index(pos).map(|index| &mut self.values[index])
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiTexture
{
    None,
    Solid,
    Text{text: String, font_size: u32},
    Custom(String),
    CustomId(TextureId)
}

impl UiTexture
{
    pub fn name(&self) -> Option<&str>
    {
        match self
        {
            Self::None
            | Self::Text{..} => None,
            Self::Solid => Some("ui/solid.png"),
            Self::Custom(x) => Some(x),
            Self::CustomId(_) => None
        }
    }
}

#[derive(Debug, Clone)]
pub struct SizeForwardInfo<SizeGet>
{
    pub parent: Option<f32>,
    pub screen_size: f32,
    pub get_element_size: SizeGet
}

#[derive(Debug, Clone, Copy)]
pub struct SizeBackwardInfo
{
    pub changes_total: bool,
    pub value: f32
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiMinimumSize
{
    Absolute(f32),
    Pixels(f32),
    FitChildren,
    FitContent(f32)
}

impl From<f32> for UiMinimumSize
{
    fn from(size: f32) -> Self
    {
        Self::Absolute(size)
    }
}

impl UiMinimumSize
{
    fn as_general<Id>(&self) -> UiSize<Id>
    {
        match self
        {
            Self::Absolute(x) => UiSize::Absolute(*x),
            Self::Pixels(x) => UiSize::Pixels(*x),
            Self::FitChildren => UiSize::FitChildren,
            Self::FitContent(x) => UiSize::FitContent(*x)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiSize<Id>
{
    Absolute(f32),
    Pixels(f32),
    FitChildren,
    FitContent(f32),
    Rest(f32),
    CopyElement(UiDirection, f32, Id)
}

impl<Id> Default for UiSize<Id>
{
    fn default() -> Self
    {
        Self::FitChildren
    }
}

impl<Id> From<f32> for UiSize<Id>
{
    fn from(size: f32) -> Self
    {
        UiSize::Absolute(size)
    }
}

impl<Id> UiSize<Id>
{
    pub fn resolve_forward<SizeGet: Fn(&UiDirection, &Id) -> Option<f32> + Copy>(
        &self,
        info: &SizeForwardInfo<SizeGet>
    ) -> Option<f32>
    {
        match self
        {
            Self::Absolute(x) => Some(*x),
            Self::Pixels(x) => Some(*x / info.screen_size),
            Self::FitChildren => None,
            Self::FitContent(_) => None,
            Self::Rest(_) => None,
            Self::CopyElement(direction, scale, id) =>
            {
                (info.get_element_size)(direction, id).map(|x| x * scale)
            }
        }
    }

    pub fn resolve_backward(
        &self,
        bounds: impl Fn() -> f32,
        parallel: bool,
        children: impl Iterator<Item=Option<SizeBackwardInfo>> + Clone
    ) -> Option<f32>
    {
        match self
        {
            Self::Absolute(x) => Some(*x),
            Self::Pixels(_) => None,
            Self::FitChildren =>
            {
                if children.clone().any(|x| x.is_none())
                {
                    return None;
                }

                let children = children.filter_map(convert::identity);

                if parallel
                {
                    Some(children.filter_map(|x| x.changes_total.then_some(x.value)).sum())
                } else
                {
                    Some(children.map(|x| x.value).max_by(|a, b| a.partial_cmp(&b).unwrap()).unwrap_or(0.0))
                }
            },
            Self::FitContent(x) => Some(bounds() * *x),
            Self::Rest(_) => None,
            Self::CopyElement(_, _, _) => None
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiDirection
{
    Horizontal,
    Vertical
}

impl UiDirection
{
    pub fn is_horizontal(&self) -> bool
    {
        if let Self::Horizontal = self
        {
            true
        } else
        {
            false
        }
    }
}

pub type UiLayout = UiDirection;

pub struct PositionResolveInfo
{
    pub this: f32,
    pub previous: f32,
    pub parent_position: f32
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiPosition<Id>
{
    Absolute(Vector2<f32>),
    Offset(Id, Vector2<f32>),
    Next(Vector2<f32>),
    Inherit
}

impl<Id> Default for UiPosition<Id>
{
    fn default() -> Self
    {
        Self::Next(Vector2::zeros())
    }
}

impl<Id> UiPosition<Id>
{
    pub fn is_inherit(&self) -> bool
    {
        if let Self::Inherit = self { true } else { false }
    }

    pub fn next_position(
        layout: &UiLayout,
        previous: Vector2<f32>,
        width: PositionResolveInfo,
        height: PositionResolveInfo
    ) -> Vector2<f32>
    {
        let position_parallel = |this: PositionResolveInfo, position|
        {
            (this.previous + this.this) / 2.0 + position
        };

        let position_perpendicular = |other: PositionResolveInfo|
        {
            other.parent_position
        };

        match layout
        {
            UiLayout::Horizontal =>
            {
                Vector2::new(position_parallel(width, previous.x), position_perpendicular(height))
            },
            UiLayout::Vertical =>
            {
                Vector2::new(position_perpendicular(width), position_parallel(height, previous.y))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedBackward
{
    pub width: Option<SizeBackwardInfo>,
    pub height: Option<SizeBackwardInfo>
}

#[derive(Debug, Clone, Copy)]
pub struct ResolvedSize
{
    pub minimum_size: Option<Option<f32>>,
    pub size: Option<f32>
}

impl Default for ResolvedSize
{
    fn default() -> Self
    {
        Self{
            minimum_size: None,
            size: None
        }
    }
}

impl ResolvedSize
{
    pub fn resolved(&self) -> bool
    {
        self.size.is_some()
    }

    pub fn value(&self) -> Option<f32>
    {
        let size = self.size?;
        if let Some(minimum) = self.minimum_size
        {
            Some(size.max(minimum?))
        } else
        {
            Some(size)
        }
    }

    pub fn unwrap(self) -> f32
    {
        self.value().unwrap()
    }

    fn as_resolved<Id>(value: Option<f32>, size: &UiSize<Id>) -> Option<f32>
    {
        if let Some(x) = value
        {
            Some(x)
        } else
        {
            if let UiSize::Rest(_) = size
            {
                Some(0.0)
            } else
            {
                None
            }
        }
    }

    pub fn resolve_backward<Id>(
        &mut self,
        bounds: impl Fn() -> f32,
        parallel: bool,
        size: &UiElementSize<Id>,
        children: impl Iterator<Item=Option<SizeBackwardInfo>> + Clone
    ) -> Option<f32>
    {
        if self.minimum_size.map(|x| x.is_none()).unwrap_or(false)
        {
            self.minimum_size = Some(size.minimum_size.as_ref().unwrap()
                .as_general::<Id>()
                .resolve_backward(&bounds, parallel, children.clone()));
        }

        if self.size.is_none()
        {
            self.size = size.size.resolve_backward(&bounds, parallel, children);
        }

        let size = Self::as_resolved(self.size.clone(), &size.size)?;

        if let Some(minimum_size) = self.minimum_size
        {
            Some(size.max(minimum_size?))
        } else
        {
            Some(size)
        }
    }

    pub fn resolve_rest<Id, T>(
        elements: &mut T,
        parallel: bool,
        parent_size: f32,
        mut output_getter: impl FnMut(&mut T, usize) -> &mut Option<f32>,
        mut minimum_size_getter: impl FnMut(&mut T, usize) -> Option<Option<f32>>,
        mut size_getter: impl FnMut(&mut T, usize) -> &UiSize<Id>,
        children: impl Iterator<Item=usize>
    )
    {
        if parallel
        {
            let mut is_ready = true;
            let mut children_size = 0.0;
            let rests = children.filter_map(|index|
            {
                let output = output_getter(elements, index);
                let is_some = output.is_some();

                children_size += output.unwrap_or(0.0);

                let value = if let UiSize::Rest(x) = size_getter(elements, index)
                {
                    let x: f32 = *x;

                    if is_some
                    {
                        is_ready = false;
                    }

                    if minimum_size_getter(elements, index).map(|x| x.is_none()).unwrap_or(false)
                    {
                        is_ready = false;
                    }

                    Some(x)
                } else
                {
                    if !is_some
                    {
                        is_ready = false;
                    }

                    None
                };

                value.map(|x| (index, x))
            }).collect::<Vec<_>>();

            if !is_ready
            {
                return;
            }

            let mut ratios_total: f32 = rests.iter().map(|(_, x)| *x).sum();

            let rest_size = |children_size, ratio, ratios_total|
            {
                (parent_size - children_size) * (ratio / ratios_total)
            };

            let mut minimums = rests.iter().filter_map(|(index, ratio)|
            {
                minimum_size_getter(elements, *index).map(|x| x.unwrap()).map(|minimum_size|
                {
                    (minimum_size, ratio)
                })
            }).collect::<Vec<_>>();
            minimums.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

            minimums.into_iter().for_each(|(minimum_size, ratio)|
            {
                let expected_size = rest_size(children_size, *ratio, ratios_total);

                if minimum_size > expected_size
                {
                    children_size += minimum_size;
                    ratios_total -= *ratio;
                }
            });

            rests.into_iter().for_each(|(index, ratio)|
            {
                let size = rest_size(children_size, ratio, ratios_total);

                *output_getter(elements, index) = Some(size);
            });
        } else
        {
            children.for_each(|index|
            {
                if let UiSize::Rest(_) = size_getter(elements, index)
                {
                    *output_getter(elements, index) = Some(parent_size);
                }
            });
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiElementSize<Id>
{
    pub minimum_size: Option<UiMinimumSize>,
    pub size: UiSize<Id>
}

impl<Id> Default for UiElementSize<Id>
{
    fn default() -> Self
    {
        Self::from(UiSize::default())
    }
}

impl<Id> From<UiSize<Id>> for UiElementSize<Id>
{
    fn from(size: UiSize<Id>) -> Self
    {
        Self{
            minimum_size: None,
            size
        }
    }
}

impl<Id> From<f32> for UiElementSize<Id>
{
    fn from(size: f32) -> Self
    {
        Self::from(UiSize::from(size))
    }
}

impl<Id> UiElementSize<Id>
{
    pub fn resolve_forward<SizeGet: Fn(&UiDirection, &Id) -> Option<f32> + Copy>(
        &self,
        info: SizeForwardInfo<SizeGet>
    ) -> ResolvedSize
    {
        ResolvedSize{
            minimum_size: self.minimum_size.as_ref().map(|x| x.as_general::<Id>().resolve_forward(&info)),
            size: self.size.resolve_forward(&info)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScalingAnimation
{
    pub start_scaling: Vector2<f32>,
    pub start_mode: Scaling,
    pub close_scaling: Vector2<f32>,
    pub close_mode: Scaling
}

impl Default for ScalingAnimation
{
    fn default() -> Self
    {
        Self{
            start_scaling: Vector2::repeat(1.0),
            start_mode: Scaling::Instant,
            close_scaling: Vector2::new(1.0, 0.0),
            close_mode: Scaling::Ignore
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionOffsets
{
    pub start: Vector2<f32>,
    pub end: Vector2<f32>
}

impl Default for PositionOffsets
{
    fn default() -> Self
    {
        Self{
            start: Vector2::zeros(),
            end: Vector2::zeros()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionAnimation
{
    pub offsets: Option<PositionOffsets>,
    pub start_mode: Connection,
    pub close_mode: Connection
}

impl Default for PositionAnimation
{
    fn default() -> Self
    {
        Self{
            offsets: None,
            start_mode: Connection::Rigid,
            close_mode: Connection::Rigid
        }
    }
}

impl PositionAnimation
{
    pub fn ease_out(decay: f32) -> Self
    {
        Self{
            offsets: None,
            start_mode: Connection::EaseOut{decay, limit: None},
            close_mode: Connection::Ignore
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Animation
{
    pub scaling: Option<ScalingAnimation>,
    pub position: Option<PositionAnimation>,
    pub mix: Option<f32>
}

impl Default for Animation
{
    fn default() -> Self
    {
        Self{
            scaling: None,
            position: None,
            mix: None
        }
    }
}

impl Animation
{
    pub fn normal() -> Self
    {
        Self{
            scaling: Some(ScalingAnimation{
                start_scaling: Vector2::new(2.0, 0.1),
                start_mode: Scaling::EaseOut{decay: 20.0},
                close_mode: Scaling::EaseOut{decay: 30.0},
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    pub fn button() -> Self
    {
        Self{
            scaling: Some(ScalingAnimation{
                start_scaling: Vector2::new(1.0, 0.1),
                start_mode: Scaling::EaseOut{decay: 20.0},
                ..Default::default()
            }),
            mix: Some(10.0),
            ..Default::default()
        }
    }

    pub fn separator_tall() -> Self
    {
        Self{
            scaling: Some(ScalingAnimation{
                start_scaling: Vector2::new(1.0, 0.01),
                start_mode: Scaling::EaseOut{decay: 30.0},
                close_scaling: Vector2::new(1.0, 0.0),
                close_mode: Scaling::EaseOut{decay: 10.0}
            }),
            ..Default::default()
        }
    }

    pub fn separator_wide() -> Self
    {
        let mut tall = Self::separator_tall();

        let scaling = tall.scaling.as_mut().unwrap();
        scaling.start_scaling = scaling.start_scaling.yx();
        scaling.close_scaling = scaling.close_scaling.yx();

        tall
    }

    pub fn scrollbar() -> Self
    {
        Self{
            scaling: Some(ScalingAnimation{
                start_scaling: Vector2::new(1.0, 0.1),
                start_mode: Scaling::EaseOut{decay: 30.0},
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    pub fn scrollbar_bar() -> Self
    {
        let mut button = Self::button();

        button.scaling.as_mut().unwrap().start_scaling.x = 1.0;

        button
    }

    pub fn typing_text() -> Self
    {
        Self{
            scaling: Some(ScalingAnimation{
                start_scaling: Vector2::new(1.0, 1.3),
                start_mode: Scaling::EaseOut{decay: 10.0},
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiElement<Id>
{
    pub texture: UiTexture,
    pub mix: Option<MixColorLch>,
    pub animation: Animation,
    pub inherit_animation: bool,
    pub position: UiPosition<Id>,
    pub children_layout: UiLayout,
    pub width: UiElementSize<Id>,
    pub height: UiElementSize<Id>,
    pub scissor: bool
}

impl<Id> Default for UiElement<Id>
{
    fn default() -> Self
    {
        Self{
            texture: UiTexture::None,
            mix: None,
            animation: Animation::default(),
            inherit_animation: true,
            position: UiPosition::default(),
            children_layout: UiLayout::Horizontal,
            width: UiElementSize::default(),
            height: UiElementSize::default(),
            scissor: false
        }
    }
}

impl<Id> UiElement<Id>
{
    pub fn fit_content() -> Self
    where
        Id: Clone
    {
        let fit_content = UiElementSize{
            size: UiSize::FitContent(1.0),
            ..Default::default()
        };

        Self{
            width: fit_content.clone(),
            height: fit_content,
            ..Default::default()
        }
    }
}
