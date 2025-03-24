use std::fmt::{self, Debug};

use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;

use crate::{
    client::{Control, ControlState, RenderCreateInfo, game_state::Ui},
    common::{
        render_info::*,
        AnyEntities,
        Entity,
        ServerToClient,
        entity::ClientEntities
    }
};


#[derive(Debug, Clone)]
pub struct MouseEvent
{
    main_button: bool,
    position: Vector2<f32>,
    state: ControlState
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct KeyboardEvent
{
    state: ControlState,
    control: yanyaengine::Control
}

#[derive(Debug, Clone)]
pub enum UiEvent
{
    MouseMove(Vector2<f32>),
    Mouse(MouseEvent),
    Keyboard(KeyboardEvent)
}

impl UiEvent
{
    pub fn as_mouse(&self) -> Option<&MouseEvent>
    {
        match self
        {
            Self::Mouse(x) => Some(x),
            _ => None
        }
    }
}

impl UiEvent
{
    pub fn from_control(
        mouse_position: Vector2<f32>,
        state: ControlState,
        control: Control
    ) -> Option<Self>
    {
        match control
        {
            Control::MainAction =>
            {
                let event = MouseEvent{main_button: true, position: mouse_position, state};
                Some(UiEvent::Mouse(event))
            },
            Control::SecondaryAction =>
            {
                let event = MouseEvent{main_button: false, position: mouse_position, state};
                Some(UiEvent::Mouse(event))
            },
            _ => None
        }
    }
}

#[derive(Debug)]
pub enum UiElementPredicate
{
    None,
    Inside(Entity)
}

impl UiElementPredicate
{
    pub fn matches(
        &self,
        entities: &ClientEntities,
        query: UiQuery,
        position: Vector2<f32>
    ) -> bool
    {
        match self
        {
            Self::None => true,
            Self::Inside(entity) =>
            {
                let transform = entities.transform(*entity).unwrap();

                query.with_transform(&transform).is_inside(position)
            }
        }
    }
}

#[derive(Debug)]
pub struct UiQuery<'a>
{
    pub shape: &'a UiElementShape,
    pub transform: &'a Transform
}

impl<'a> UiQuery<'a>
{
    pub fn with_transform(self, transform: &'a Transform) -> Self
    {
        Self{
            transform,
            ..self
        }
    }

    pub fn relative_position(&self) -> Vector2<f32>
    {
        self.transform.position.xy()
    }

    pub fn distance(&self, position: Vector2<f32>) -> Vector2<f32>
    {
        (self.relative_position() - position).component_div(&self.transform.scale.xy())
    }

    pub fn is_inside(&self, position: Vector2<f32>) -> bool
    {
        self.shape.is_inside(
            self.transform.scale.xy(),
            position - self.relative_position()
        )
    }
}

#[derive(Debug)]
pub enum AspectMode
{
    ShrinkX,
    FillRestX
}

#[derive(Debug)]
pub enum AspectPosition
{
    UiScaled(Vector2<f32>),
    Absolute(Vector2<f32>)
}

#[derive(Debug)]
pub struct KeepAspect
{
    pub scale: Vector2<f32>,
    pub position: AspectPosition,
    pub mode: AspectMode,
}

impl Default for KeepAspect
{
    fn default() -> Self
    {
        Self{
            scale: Vector2::repeat(1.0),
            position: AspectPosition::UiScaled(Vector2::zeros()),
            mode: AspectMode::ShrinkX,
        }
    }
}

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
    Text{text: String, font_size: u32, font: FontStyle, align: TextAlign},
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
pub struct SizeForwardInfo
{
    pub parent: Option<f32>
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
    FitContent
}

impl UiMinimumSize
{
    fn as_general(&self) -> UiSize
    {
        match self
        {
            Self::Absolute(x) => UiSize::Absolute(*x),
            Self::FitChildren => UiSize::FitChildren,
            Self::FitContent => UiSize::FitContent
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiSize
{
    ParentScale(f32),
    Absolute(f32),
    FitChildren,
    FitContent
}

impl Default for UiSize
{
    fn default() -> Self
    {
        Self::ParentScale(1.0)
    }
}

impl UiSize
{
    pub fn resolve_forward(&self, info: &SizeForwardInfo) -> Option<f32>
    {
        match self
        {
            Self::ParentScale(fraction) => info.parent.map(|x| x * fraction),
            Self::Absolute(x) => Some(*x),
            Self::FitChildren => None,
            Self::FitContent => None
        }
    }

    pub fn resolve_backward(
        &self,
        bounds: impl Fn() -> f32,
        children: impl Iterator<Item=SizeBackward>
    ) -> Option<f32>
    {
        match self
        {
            Self::ParentScale(_) => None,
            Self::Absolute(x) => Some(*x),
            Self::FitChildren =>
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
            },
            Self::FitContent => Some(bounds())
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

    fn value(&self) -> Option<f32>
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

    fn as_resolved(value: Option<f32>, size: &UiSize) -> SizeBackward
    {
        if let Some(x) = value
        {
            SizeBackward::Value(x)
        } else
        {
            if let UiSize::ParentScale(x) = size
            {
                SizeBackward::ParentRelative(*x)
            } else
            {
                unreachable!()
            }
        }
    }

    pub fn resolve_backward(
        &mut self,
        bounds: impl Fn() -> f32,
        size: &UiElementSize,
        children: impl Iterator<Item=SizeBackwardInfo> + Clone
    ) -> SizeBackwardInfo
    {
        if self.minimum_size.is_none()
        {
            self.minimum_size = size.minimum_size.as_ref().map(|x|
            {
                x.as_general().resolve_backward(&bounds, children.clone()).unwrap()
            });
        }

        if self.size.is_none()
        {
            self.size = size.size.resolve_backward(&bounds, children);
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiElementSize
{
    pub minimum_size: Option<UiMinimumSize>,
    pub size: UiSize
}

impl Default for UiElementSize
{
    fn default() -> Self
    {
        Self{
            minimum_size: None,
            size: UiSize::default()
        }
    }
}

impl UiElementSize
{
    pub fn resolve_forward(&self, info: SizeForwardInfo) -> ResolvedSize
    {
        ResolvedSize{
            minimum_size: self.minimum_size.as_ref().and_then(|x| x.as_general().resolve_forward(&info)),
            size: self.size.resolve_forward(&info)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiElement
{
    pub texture: UiTexture,
    pub mix: Option<MixColor>,
    pub width: UiElementSize,
    pub height: UiElementSize
}

impl Default for UiElement
{
    fn default() -> Self
    {
        Self{
            texture: UiTexture::Solid,
            mix: None,
            width: UiElementSize::default(),
            height: UiElementSize::default()
        }
    }
}

impl UiElement
{
    pub fn fit_content() -> Self
    {
        let fit_content = UiElementSize{
            size: UiSize::FitContent,
            ..Default::default()
        };

        Self{
            width: fit_content.clone(),
            height: fit_content,
            ..Default::default()
        }
    }
}
