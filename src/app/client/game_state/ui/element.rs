use std::{
    rc::Rc,
    cell::RefCell,
    fmt::Debug
};

use nalgebra::Vector2;

use crate::common::{
    some_or_value,
    render_info::*,
    lazy_transform::*
};

pub use crate::common::lazy_transform::Scaling;


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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiElementShape
{
    Rectangle,
    Mask(UiElementShapeMask)
}

impl UiElementShape
{
    pub fn is_inside(&self, scale: Vector2<f32>, position: Vector2<f32>) -> bool
    {
        match self
        {
            Self::Rectangle =>
            {
                let inbounds = |half_size: f32, pos: f32| -> bool
                {
                    (-half_size..=half_size).contains(&pos)
                };

                let half_scale = scale / 2.0;

                inbounds(half_scale.x, position.x)
                    && inbounds(half_scale.y, position.y)
            },
            Self::Mask(x) => x.is_inside(position.component_div(&scale) + Vector2::repeat(0.5))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiTexture
{
    None,
    Solid,
    Text{text: String, font_size: u32, font: FontStyle, align: Option<TextAlign>},
    Custom(String)
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
            Self::Custom(x) => Some(x)
        }
    }
}

#[derive(Debug, Clone)]
pub struct SizeForwardInfo<SizeGet>
{
    pub parent: Option<f32>,
    pub get_element_size: SizeGet
}

#[derive(Debug, Clone)]
pub enum SizeBackward
{
    ParentRelative(f32),
    Value(f32)
}

impl SizeBackward
{
    fn max(self, other: f32) -> Self
    {
        match self
        {
            Self::Value(x) => Self::Value(x + other),
            _ => panic!("cant solve minimum size constraint")
        }
    }
}

pub type SizeBackwardInfo = SizeBackward;

#[derive(Debug, Clone, PartialEq)]
pub enum UiMinimumSize
{
    Absolute(f32),
    FitChildren,
    FitContent(f32)
}

impl UiMinimumSize
{
    fn as_general<Id>(&self) -> UiSize<Id>
    {
        match self
        {
            Self::Absolute(x) => UiSize::Absolute(*x),
            Self::FitChildren => UiSize::FitChildren,
            Self::FitContent(x) => UiSize::FitContent(*x)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiSize<Id>
{
    ParentScale(f32),
    Absolute(f32),
    FitChildren,
    FitContent(f32),
    Rest(f32),
    CopyElement(UiDirection, Id)
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
            Self::ParentScale(fraction) => info.parent.map(|x| x * fraction),
            Self::Absolute(x) => Some(*x),
            Self::FitChildren => None,
            Self::FitContent(_) => None,
            Self::Rest(_) => None,
            Self::CopyElement(direction, id) =>
            {
                Some(some_or_value!((info.get_element_size)(direction, id), Some(0.0)))
            }
        }
    }

    pub fn resolve_backward(
        &self,
        bounds: impl Fn() -> f32,
        parallel: bool,
        children: impl Iterator<Item=SizeBackward>
    ) -> Option<f32>
    {
        match self
        {
            Self::ParentScale(_) => None,
            Self::Absolute(x) => Some(*x),
            Self::FitChildren =>
            {
                if parallel
                {
                    let (sum_normal, sum_relative) = children.fold(
                        (0.0, 0.0),
                        |(sum_normal, sum_relative), info|
                        {
                            match info
                            {
                                SizeBackward::ParentRelative(x) => (sum_normal, sum_relative + x),
                                SizeBackward::Value(x) => (sum_normal + x, sum_relative)
                            }
                        });

                    assert!(sum_relative < 1.0);

                    let leftover = 1.0 - sum_relative;

                    Some(sum_normal / leftover)
                } else
                {
                    Some(children.filter_map(|info|
                    {
                        match info
                        {
                            SizeBackward::Value(x) => Some(x),
                            _ => None
                        }
                    }).max_by(|a, b| a.partial_cmp(&b).unwrap()).unwrap_or(0.0))
                }
            },
            Self::FitContent(x) => Some(bounds() * *x),
            Self::Rest(_) => None,
            Self::CopyElement(_, _) => None
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
pub enum UiPosition
{
    Absolute(Vector2<f32>),
    Next
}

impl Default for UiPosition
{
    fn default() -> Self
    {
        Self::Next
    }
}

impl UiPosition
{
    pub fn resolve_forward(
        &self,
        layout: &UiLayout,
        previous: Vector2<f32>,
        width: PositionResolveInfo,
        height: PositionResolveInfo
    ) -> Vector2<f32>
    {
        match self
        {
            Self::Absolute(_) => unreachable!(),
            Self::Next =>
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
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedBackward
{
    pub width: SizeBackwardInfo,
    pub height: SizeBackwardInfo
}

#[derive(Debug, Clone, Copy)]
pub struct ResolvedSize
{
    pub minimum_size: Option<f32>,
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
            Some(size.max(minimum))
        } else
        {
            Some(size)
        }
    }

    pub fn unwrap(self) -> f32
    {
        self.value().unwrap()
    }

    fn as_resolved<Id>(value: Option<f32>, size: &UiSize<Id>) -> SizeBackward
    {
        if let Some(x) = value
        {
            SizeBackward::Value(x)
        } else
        {
            if let UiSize::ParentScale(x) = size
            {
                SizeBackward::ParentRelative(*x)
            } else if let UiSize::Rest(_) = size
            {
                SizeBackward::Value(0.0)
            } else
            {
                unreachable!()
            }
        }
    }

    pub fn resolve_backward<Id>(
        &mut self,
        bounds: impl Fn() -> f32,
        parallel: bool,
        size: &UiElementSize<Id>,
        children: impl Iterator<Item=SizeBackwardInfo> + Clone
    ) -> SizeBackwardInfo
    {
        if self.minimum_size.is_none()
        {
            self.minimum_size = size.minimum_size.as_ref().map(|x|
            {
                x.as_general::<Id>().resolve_backward(&bounds, parallel, children.clone()).unwrap()
            });
        }

        if self.size.is_none()
        {
            self.size = size.size.resolve_backward(&bounds, parallel, children);
        }

        let size = Self::as_resolved(self.size.clone(), &size.size);

        if let Some(minimum_size) = self.minimum_size
        {
            size.max(minimum_size)
        } else
        {
            size
        }
    }

    pub fn resolve_children<'a, 'b, Id: 'b>(&self, children: impl Iterator<Item=(&'a mut Option<f32>, &'b UiSize<Id>)>)
    {
        if let Some(parent_size) = self.value()
        {
            let mut children_size = 0.0;
            let rests = children.filter(|(size, x)|
            {
                children_size += size.unwrap_or(0.0);
                if let UiSize::Rest(_) = x { true } else { false }
            }).collect::<Vec<_>>();

            if rests.iter().any(|(x, _)| x.is_some())
            {
                return;
            }

            let ratios_total: f32 = rests.iter().map(|(_, x)| if let UiSize::Rest(x) = x { x } else { unreachable!() })
                .sum();

            rests.into_iter().for_each(|(value, ratio)|
            {
                let ratio = if let UiSize::Rest(x) = ratio { x } else { unreachable!() };
                let size = (parent_size - children_size) * (ratio / ratios_total);

                *value = Some(size);
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
            minimum_size: self.minimum_size.as_ref().and_then(|x| x.as_general::<Id>().resolve_forward(&info)),
            size: self.size.resolve_forward(&info)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScalingAnimation
{
    pub start_scaling: Vector2<f32>,
    pub start_mode: Scaling,
    pub close_mode: Scaling
}

#[derive(Debug, Clone, PartialEq)]
pub struct Animation
{
    pub scaling: Option<ScalingAnimation>,
    pub mix: Option<f32>
}

impl Default for Animation
{
    fn default() -> Self
    {
        Self{
            scaling: None,
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
                close_mode: Scaling::EaseOut{decay: 30.0}
            }),
            ..Default::default()
        }
    }

    pub fn text() -> Self
    {
        Self{
            scaling: Some(ScalingAnimation{
                start_scaling: Vector2::repeat(0.5),
                start_mode: Scaling::EaseOut{decay: 20.0},
                close_mode: Scaling::EaseOut{decay: 30.0}
            }),
            ..Default::default()
        }
    }

    pub fn button() -> Self
    {
        Self{
            scaling: Some(ScalingAnimation{
                start_scaling: Vector2::repeat(0.5),
                start_mode: Scaling::EaseOut{decay: 20.0},
                close_mode: Scaling::EaseOut{decay: 30.0}
            }),
            mix: Some(10.0),
            ..Default::default()
        }
    }

    pub fn scrollbar() -> Self
    {
        Self{
            scaling: Some(ScalingAnimation{
                start_scaling: Vector2::repeat(0.1),
                start_mode: Scaling::EaseOut{decay: 30.0},
                close_mode: Scaling::EaseOut{decay: 30.0}
            }),
            ..Default::default()
        }
    }

    pub fn scrollbar_bar() -> Self
    {
        Self::button()
    }

    pub fn typing_text() -> Self
    {
        Self{
            scaling: Some(ScalingAnimation{
                start_scaling: Vector2::new(1.1, 1.1),
                start_mode: Scaling::EaseOut{decay: 10.0},
                close_mode: Scaling::EaseIn(EaseInScaling::new(1.0))
            }),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiElement<Id>
{
    pub texture: UiTexture,
    pub mix: Option<MixColor>,
    pub animation: Animation,
    pub position: UiPosition,
    pub children_layout: UiLayout,
    pub width: UiElementSize<Id>,
    pub height: UiElementSize<Id>
}

impl<Id> Default for UiElement<Id>
{
    fn default() -> Self
    {
        Self{
            texture: UiTexture::None,
            mix: None,
            animation: Animation::default(),
            position: UiPosition::default(),
            children_layout: UiLayout::Horizontal,
            width: UiElementSize::default(),
            height: UiElementSize::default()
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
