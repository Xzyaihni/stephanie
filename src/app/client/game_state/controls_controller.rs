use std::{
    mem,
    error,
    fmt::{self, Display}
};

use yanyaengine::{ElementState, PhysicalKey, KeyCode, KeyCodeNamed, MouseButton};

use strum::EnumCount;

use clipboard::{ClipboardProvider, ClipboardContext};

use crate::common::BiMap;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumCount)]
pub enum Control
{
    MoveUp = 0,
    MoveDown,
    MoveRight,
    MoveLeft,
    MainAction,
    SecondaryAction,
    Interact,
    Jump,
    Crawl,
    Sprint,
    Poke,
    Shoot,
    Throw,
    Inventory,
    ZoomIn,
    ZoomOut,
    ZoomReset
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

impl ControlState
{
    pub fn to_bool(self) -> bool
    {
        match self
        {
            Self::Released => false,
            Self::Pressed => true
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum KeyMapping
{
    Keyboard(KeyCode),
    Mouse(MouseButton)
}

impl Display for KeyMapping
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::Keyboard(key) =>
            {
                write!(f, "{}", KeyCodeNamed(*key).to_string().trim_start_matches("Key"))
            },
            Self::Mouse(button) =>
            {
                write!(f, "{}", match button
                {
                    MouseButton::Left => "Left mouse",
                    MouseButton::Right => "Right mouse",
                    MouseButton::Middle => "Middle mouse",
                    _ => "unknown"
                })
            }
        }
    }
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
    clipboard: Option<ClipboardContext>,
    key_mapping: BiMap<KeyMapping, Control>,
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
            (KeyMapping::Keyboard(KeyCode::KeyC), Control::MainAction),
            (KeyMapping::Mouse(MouseButton::Right), Control::SecondaryAction),
            (KeyMapping::Keyboard(KeyCode::KeyV), Control::SecondaryAction),
            (KeyMapping::Keyboard(KeyCode::KeyE), Control::Interact),
            (KeyMapping::Keyboard(KeyCode::Space), Control::Jump),
            (KeyMapping::Keyboard(KeyCode::ControlLeft), Control::Crawl),
            (KeyMapping::Keyboard(KeyCode::ShiftLeft), Control::Sprint),
            (KeyMapping::Keyboard(KeyCode::KeyF), Control::Shoot),
            (KeyMapping::Keyboard(KeyCode::KeyG), Control::Poke),
            (KeyMapping::Keyboard(KeyCode::KeyI), Control::Inventory),
            (KeyMapping::Keyboard(KeyCode::KeyT), Control::Throw),
            (KeyMapping::Keyboard(KeyCode::Equal), Control::ZoomIn),
            (KeyMapping::Keyboard(KeyCode::Minus), Control::ZoomOut),
            (KeyMapping::Keyboard(KeyCode::Digit0), Control::ZoomReset)
        ].into_iter().collect();

        let clipboard = match ClipboardProvider::new()
        {
            Ok(x) => Some(x),
            Err(err) =>
            {
                eprintln!("error getting clipboard: {err}");

                None
            }
        };

        Self{
            clipboard,
            key_mapping,
            keys: [ControlState::Released; Control::COUNT],
            changed: Vec::new()
        }
    }

    pub fn get_clipboard(&mut self) -> Result<String, Box<dyn error::Error>>
    {
        self.clipboard.as_mut().ok_or_else(||
        {
            "clipboard not initialized".into()
        }).and_then(|clipboard| clipboard.get_contents())
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

    pub fn key_for(&self, control: &Control) -> Option<&KeyMapping>
    {
        self.key_mapping.get_back(control)
    }

    pub fn changed_this_frame(&mut self) -> Vec<(ControlState, Control)>
    {
        mem::take(&mut self.changed)
    }
}
