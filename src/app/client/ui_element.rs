use std::fmt::{self, Debug};

use strum::AsRefStr;

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

#[derive(Debug, Clone)]
pub enum UiTexture
{
    Solid
}

#[derive(Debug)]
pub struct UiElement
{
    pub texture: UiTexture
}

impl Default for UiElement
{
    fn default() -> Self
    {
        Self{
            texture: UiTexture::Solid
        }
    }
}
