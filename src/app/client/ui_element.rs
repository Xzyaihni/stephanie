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

#[derive(Default)]
pub struct DragState
{
    held: bool
}

pub enum UiElementType
{
    Panel,
    Button{on_click: Box<dyn FnMut()>},
    Drag{state: DragState, on_change: Box<dyn FnMut(Vector2<f32>)>}
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
            UiElementType::Drag{state, on_change} =>
            {
                let inner_position = |position|
                {
                    distance(position).map(|x| x.clamp(-0.5, 0.5))
                };

                match event
                {
                    UiEvent::Mouse(event) =>
                    {
                        if event.main_button
                        {
                            if event.state == ControlState::Pressed && is_inside(event.position)
                            {
                                on_change(inner_position(event.position));

                                state.held = true;

                                return true;
                            }

                            if event.state == ControlState::Released
                            {
                                state.held = false;

                                return true;
                            }
                        }
                    },
                    UiEvent::MouseMove(position) =>
                    {
                        if state.held
                        {
                            on_change(inner_position(*position));
                        }
                    },
                    _ => ()
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
