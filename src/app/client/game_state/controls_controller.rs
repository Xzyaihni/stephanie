use std::{
    mem,
    collections::HashMap
};

use yanyaengine::{ElementState, PhysicalKey, KeyCode, MouseButton};

use strum::EnumCount;


#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumCount)]
pub enum Control
{
    MoveUp = 0,
    MoveDown,
    MoveRight,
    MoveLeft,
    MainAction,
    SecondaryAction,
    Jump,
    Crouch,
    Poke,
    Shoot,
    Throw,
    Inventory,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    DebugConsole
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlState
{
    Released,
    Pressed
}

impl From<&yanyaengine::Control> for ControlState
{
    fn from(value: &yanyaengine::Control) -> Self
    {
        let estate_to_state = |estate: &ElementState|
        {
            match *estate
            {
                ElementState::Pressed => ControlState::Pressed,
                ElementState::Released => ControlState::Released
            }
        };

        match value
        {
            yanyaengine::Control::Keyboard{state, ..} => estate_to_state(state),
            yanyaengine::Control::Mouse{state, ..} => estate_to_state(state),
            yanyaengine::Control::Scroll{..} => ControlState::Pressed
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum KeyMapping
{
    Keyboard(KeyCode),
    Mouse(MouseButton)
}

impl KeyMapping
{
    pub fn from_control(value: yanyaengine::Control) -> Option<Self>
    {
        match value
        {
            yanyaengine::Control::Keyboard{
                keycode: PhysicalKey::Code(code),
                ..
            } => Some(KeyMapping::Keyboard(code)),
            yanyaengine::Control::Mouse{button, ..} => Some(KeyMapping::Mouse(button)),
            yanyaengine::Control::Scroll{..} => None,
            _ => None
        }
    }
}

pub struct ControlsController
{
    key_mapping: HashMap<KeyMapping, Control>,
    keys: [ControlState; Control::COUNT],
    changed: Vec<(ControlState, Control)>
}

impl ControlsController
{
    pub fn new() -> Self
    {
        let key_mapping = [
            (KeyMapping::Keyboard(KeyCode::KeyD), Control::MoveRight),
            (KeyMapping::Keyboard(KeyCode::KeyA), Control::MoveLeft),
            (KeyMapping::Keyboard(KeyCode::KeyS), Control::MoveDown),
            (KeyMapping::Keyboard(KeyCode::KeyW), Control::MoveUp),
            (KeyMapping::Mouse(MouseButton::Left), Control::MainAction),
            (KeyMapping::Mouse(MouseButton::Right), Control::SecondaryAction),
            (KeyMapping::Keyboard(KeyCode::KeyV), Control::SecondaryAction),
            (KeyMapping::Keyboard(KeyCode::Space), Control::Jump),
            (KeyMapping::Keyboard(KeyCode::ControlLeft), Control::Crouch),
            (KeyMapping::Keyboard(KeyCode::KeyF), Control::Shoot),
            (KeyMapping::Keyboard(KeyCode::KeyG), Control::Poke),
            (KeyMapping::Keyboard(KeyCode::KeyI), Control::Inventory),
            (KeyMapping::Keyboard(KeyCode::KeyT), Control::Throw),
            (KeyMapping::Keyboard(KeyCode::Equal), Control::ZoomIn),
            (KeyMapping::Keyboard(KeyCode::Minus), Control::ZoomOut),
            (KeyMapping::Keyboard(KeyCode::Digit0), Control::ZoomReset),
            (KeyMapping::Keyboard(KeyCode::Backquote), Control::DebugConsole)
        ].into_iter().collect();

        Self{
            key_mapping,
            keys: [ControlState::Released; Control::COUNT],
            changed: Vec::new()
        }
    }

    pub fn is_down(&self, control: Control) -> bool
    {
        match self.state(control)
        {
            ControlState::Pressed => true,
            _ => false
        }
    }

    pub fn is_up(&self, control: Control) -> bool
    {
        !self.is_down(control)
    }

    pub fn state(&self, control: Control) -> ControlState
    {
        self.keys[control as usize]
    }

    pub fn handle_input(&mut self, input: yanyaengine::Control) -> Option<(ControlState, Control)>
    {
        let state = ControlState::from(&input);

        let this_key = KeyMapping::from_control(input);
        
        if let Some(this_key) = this_key
        {
            let matched = self.key_mapping.get(&this_key);

            if let Some(matched) = matched
            {
                self.keys[*matched as usize] = state;

                let pair = (state, *matched);
                self.changed.push(pair);

                return Some(pair);
            }
        }

        None
    }

    pub fn changed_this_frame(&mut self) -> Vec<(ControlState, Control)>
    {
        mem::take(&mut self.changed)
    }
}
