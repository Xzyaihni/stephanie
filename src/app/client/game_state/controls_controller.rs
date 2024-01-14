use std::collections::HashMap;

use yanyaengine::{ElementState, PhysicalKey, KeyCode, MouseButton};

use enum_amount::EnumCount;


#[repr(usize)]
#[derive(Debug, Clone, Copy, EnumCount)]
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
	ZoomIn,
	ZoomOut,
	ZoomReset,
    DebugConsole
}

#[derive(Debug, Clone, Copy)]
pub enum ControlState
{
    Released,
    Held,
    Clicked
}

impl From<&yanyaengine::Control> for ControlState
{
    fn from(value: &yanyaengine::Control) -> Self
    {
        let estate_to_state = |estate: &ElementState|
        {
            match *estate
            {
                ElementState::Pressed => ControlState::Clicked,
                ElementState::Released => ControlState::Released
            }
        };

        match value
        {
            yanyaengine::Control::Keyboard{state, ..} => estate_to_state(state),
            yanyaengine::Control::Mouse{state, ..} => estate_to_state(state),
            yanyaengine::Control::Scroll{..} => ControlState::Clicked
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
    keys: [ControlState; Control::COUNT]
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
			(KeyMapping::Keyboard(KeyCode::Space), Control::Jump),
			(KeyMapping::Keyboard(KeyCode::ControlLeft), Control::Crouch),
			(KeyMapping::Keyboard(KeyCode::Equal), Control::ZoomIn),
			(KeyMapping::Keyboard(KeyCode::Minus), Control::ZoomOut),
			(KeyMapping::Keyboard(KeyCode::Digit0), Control::ZoomReset),
			(KeyMapping::Keyboard(KeyCode::Backquote), Control::DebugConsole)
        ].into_iter().collect();

        Self{
            key_mapping,
            keys: [ControlState::Released; Control::COUNT]
        }
    }

    pub fn state(&self, control: Control) -> ControlState
    {
        self.keys[control as usize]
    }

    pub fn handle_input(&mut self, input: yanyaengine::Control)
    {
        let state = ControlState::from(&input);

        let this_key = KeyMapping::from_control(input);
        
        if let Some(this_key) = this_key
        {
            let matched = self.key_mapping.get(&this_key);

            if let Some(matched) = matched
            {
                self.keys[*matched as usize] = state;
            }
        }
    }

    pub fn release_clicked(&mut self)
    {
        self.keys.iter_mut().for_each(|key|
        {
            if let ControlState::Clicked = *key
            {
                *key = ControlState::Held;
            }
        });
    }
}
