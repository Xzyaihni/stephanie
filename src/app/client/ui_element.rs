use std::{
    fmt::{self, Debug},
    cell::Ref
};

use strum::AsRefStr;

use nalgebra::Vector2;

use yanyaengine::Transform;

use crate::{
    client::{Control, ControlState, RenderCreateInfo, game_state::{close_ui, Ui}},
    common::{render_info::*, Entity, ServerToClient, entity::ClientEntities}
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

#[derive(Default)]
pub struct DragState
{
    held: bool
}

#[derive(AsRefStr)]
pub enum UiElementType
{
    Panel,
    Tooltip,
    ActiveTooltip,
    Button{on_click: Box<dyn FnMut()>},
    Drag{state: DragState, on_change: Box<dyn FnMut(Vector2<f32>)>}
}

impl Debug for UiElementType
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "UiElementType::{}", self.as_ref())
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

                query.with_transform(transform).is_inside(position)
            }
        }
    }
}

#[derive(Debug)]
pub struct UiQuery<'a>
{
    pub transform: Ref<'a, Transform>,
    pub camera_position: Vector2<f32>
}

impl<'a> UiQuery<'a>
{
    pub fn with_transform(self, transform: Ref<'a, Transform>) -> Self
    {
        Self{
            transform,
            ..self
        }
    }

    pub fn relative_position(&self) -> Vector2<f32>
    {
        self.transform.position.xy() - self.camera_position
    }

    pub fn distance(&self, position: Vector2<f32>) -> Vector2<f32>
    {
        (self.relative_position() - position).component_div(&self.transform.scale.xy())
    }

    pub fn is_inside(&self, position: Vector2<f32>) -> bool
    {
        UiElement::is_inside(self.transform.scale.xy(), self.relative_position() - position)
    }
}

#[derive(Debug)]
pub enum AspectMode
{
    ShrinkX,
    FillRestX
}

#[derive(Debug)]
pub struct KeepAspect
{
    pub scale: Vector2<f32>,
    pub position: Vector2<f32>,
    pub mode: AspectMode,
}

impl Default for KeepAspect
{
    fn default() -> Self
    {
        Self{
            scale: Vector2::repeat(1.0),
            position: Vector2::zeros(),
            mode: AspectMode::ShrinkX,
        }
    }
}

#[derive(Debug)]
pub struct UiElement
{
    pub kind: UiElementType,
    pub predicate: UiElementPredicate,
    pub keep_aspect: Option<KeepAspect>
}

impl Default for UiElement
{
    fn default() -> Self
    {
        Self{
            kind: UiElementType::Panel,
            predicate: UiElementPredicate::None,
            keep_aspect: None
        }
    }
}

impl ServerToClient<UiElement> for ()
{
    fn server_to_client(
        self,
        _transform: impl FnOnce() -> Transform,
        _create_info: &mut RenderCreateInfo
    ) -> UiElement
    {
        unreachable!()
    }
}

impl UiElement
{
    pub fn update(
        &mut self,
        entities: &ClientEntities,
        entity: Entity,
        camera_position: Vector2<f32>,
        event: &UiEvent,
        captured: bool
    ) -> bool
    {
        let query = ||
        {
            UiQuery{transform: entities.transform(entity).unwrap(), camera_position}
        };

        let highlight = |state: bool|
        {
            if let Some(mut lazy_mix) = entities.lazy_mix_mut(entity)
            {
                lazy_mix.target.amount = if state { 0.4 } else { 0.0 };
            }
        };

        let position = match event
        {
            UiEvent::MouseMove(position) => Some(*position),
            UiEvent::Mouse(event) => Some(event.position),
            UiEvent::Keyboard(..) => None
        };

        let is_inside = position.map(|position| query().is_inside(position));
        let predicate = position.map(|position|
        {
            self.predicate.matches(entities, query(), position)
        }).unwrap_or(false);

        let capture_this = is_inside.unwrap_or(false);

        match &self.kind
        {
            UiElementType::Button{..} | UiElementType::Drag{..} =>
            {
                highlight(capture_this && !captured && predicate);
            },
            UiElementType::Panel | UiElementType::Tooltip | UiElementType::ActiveTooltip => ()
        }

        let remove_this = ||
        {
            close_ui(entities, entity);
        };

        match &mut self.kind
        {
            UiElementType::Panel =>
            {
                if captured
                {
                    return true;
                }
            },
            UiElementType::Tooltip =>
            {
                if let Some(is_inside) = is_inside
                {
                    if !is_inside
                    {
                        remove_this();
                    }
                }

                if captured
                {
                    return true;
                }
            },
            UiElementType::ActiveTooltip =>
            {
                if captured
                {
                    return true;
                }

                match event
                {
                    UiEvent::MouseMove(_) => (),
                    UiEvent::Mouse(event) =>
                    {
                        if !capture_this && event.state == ControlState::Pressed
                        {
                            remove_this();
                        }
                    }, UiEvent::Keyboard(..) =>
                    {
                        if !capture_this
                        {
                            remove_this();
                        }
                    }
                }
            },
            UiElementType::Button{on_click} =>
            {
                if captured
                {
                    return true;
                }

                if let Some(event) = event.as_mouse()
                {
                    let clicked = event.main_button && event.state == ControlState::Pressed;

                    if clicked && query().is_inside(event.position)
                    {
                        if !predicate
                        {
                            return false;
                        }

                        on_click();
                    }
                }
            },
            UiElementType::Drag{state, on_change} =>
            {
                let inner_position = |position|
                {
                    query().distance(position).map(|x| x.clamp(-0.5, 0.5))
                };

                match event
                {
                    UiEvent::Mouse(event) =>
                    {
                        if event.main_button
                        {
                            match event.state
                            {
                                ControlState::Pressed =>
                                {
                                    if !captured
                                        && query().is_inside(event.position)
                                    {
                                        if !predicate
                                        {
                                            return false;
                                        }

                                        on_change(inner_position(event.position));

                                        state.held = true;
                                    }
                                },
                                ControlState::Released =>
                                {
                                    if event.state == ControlState::Released
                                    {
                                        state.held = false;
                                    }
                                }
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
            }
        }

        capture_this
    }

    pub fn update_aspect(
        &mut self,
        transform: &mut Transform,
        render: &mut ClientRenderInfo,
        aspect: f32
    )
    {
        if let Some(keep_aspect) = &self.keep_aspect
        {
            transform.scale.x = match keep_aspect.mode
            {
                AspectMode::ShrinkX =>
                {
                    keep_aspect.scale.x / aspect
                },
                AspectMode::FillRestX =>
                {
                    1.0 - keep_aspect.scale.x / aspect
                }
            };

            transform.scale.y = keep_aspect.scale.y;

            transform.position = Ui::ui_position(
                transform.scale,
                keep_aspect.position.xyy()
            );

            render.set_transform(transform.clone());
        }
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
