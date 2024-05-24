use nalgebra::Vector2;

use yanyaengine::{Transform, game_object::*};

use crate::{
    client::{Control, ControlState},
    common::ServerToClient
};


#[derive(Debug, Clone)]
pub struct MouseEvent
{
    main_button: bool,
    position: Vector2<f32>,
    state: ControlState
}

#[derive(Debug, Clone)]
pub struct KeyboardEvent
{
    state: ControlState,
    control: yanyaengine::Control
}

#[derive(Debug, Clone)]
pub enum UiEvent
{
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
        mouse_position: impl FnOnce() -> Vector2<f32>,
        state: ControlState,
        control: Control
    ) -> Option<Self>
    {
        match control
        {
            Control::MainAction =>
            {
                let event = MouseEvent{main_button: true, position: mouse_position(), state};
                Some(UiEvent::Mouse(event))
            },
            Control::SecondaryAction =>
            {
                let event = MouseEvent{main_button: false, position: mouse_position(), state};
                Some(UiEvent::Mouse(event))
            },
            _ => None
        }
    }
}

pub enum UiElementType
{
    Panel,
    Button{on_click: Box<dyn FnMut()>},
    Drag{on_change: Box<dyn FnMut(Vector2<f32>)>}
}

pub struct UiElement
{
    pub kind: UiElementType
}

impl ServerToClient<UiElement> for ()
{
    fn server_to_client(
        self,
        _transform: impl FnOnce() -> Transform,
        _create_info: &mut ObjectCreateInfo
    ) -> UiElement
    {
        unreachable!()
    }
}

impl UiElement
{
    pub fn update(
        &mut self,
        distance: impl Fn(Vector2<f32>) -> Vector2<f32>,
        is_inside: impl Fn(Vector2<f32>) -> bool,
        event: &UiEvent
    ) -> bool
    {
        match &mut self.kind
        {
            UiElementType::Panel => false,
            UiElementType::Button{on_click} =>
            {
                if let Some(event) = event.as_mouse()
                {
                    let clicked = event.main_button && event.state == ControlState::Pressed;
                    if clicked && is_inside(event.position)
                    {
                        on_click();

                        return true;
                    }
                }

                false
            },
            UiElementType::Drag{on_change} =>
            {
                if let Some(event) = event.as_mouse()
                {
                    let down = event.main_button && event.state == ControlState::Pressed;
                    if down && is_inside(event.position)
                    {
                        on_change(distance(event.position));

                        return true;
                    }
                }

                false
            }
        }
    }

    pub fn distance(
        element_position: Vector2<f32>,
        camera_position: Vector2<f32>,
        position: Vector2<f32>
    ) -> Vector2<f32>
    {
        let offset = element_position - camera_position;

        offset.xy() - position
    }

    pub fn is_inside(scale: Vector2<f32>, position: Vector2<f32>) -> bool
    {
        let inbounds = |half_size: f32, pos: f32| -> bool
        {
            (-half_size..=half_size).contains(&pos)
        };

        let half_scale = scale / 2.0;

        inbounds(half_scale.x, position.x)
            && inbounds(half_scale.y, position.y)
    }
}
