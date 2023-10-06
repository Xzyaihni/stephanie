use std::collections::HashMap;

use winit::event::{ElementState, VirtualKeyCode};

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
	ZoomReset
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
    Keyboard(VirtualKeyCode),
    Mouse(u32)
}

impl KeyMapping
{
    pub fn from_control(value: yanyaengine::Control) -> Option<Self>
    {
        match value
        {
            yanyaengine::Control::Keyboard{keycode, ..} => Some(KeyMapping::Keyboard(keycode)),
            yanyaengine::Control::Mouse{button, ..} => Some(KeyMapping::Mouse(button)),
            yanyaengine::Control::Scroll{..} => None
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
            (KeyMapping::Keyboard(VirtualKeyCode::D), Control::MoveRight),
            (KeyMapping::Keyboard(VirtualKeyCode::A), Control::MoveLeft),
            (KeyMapping::Keyboard(VirtualKeyCode::S), Control::MoveDown),
            (KeyMapping::Keyboard(VirtualKeyCode::W), Control::MoveUp),
            (KeyMapping::Mouse(3), Control::MainAction),
            (KeyMapping::Mouse(1), Control::SecondaryAction),
			(KeyMapping::Keyboard(VirtualKeyCode::Space), Control::Jump),
			(KeyMapping::Keyboard(VirtualKeyCode::LControl), Control::Crouch),
			(KeyMapping::Keyboard(VirtualKeyCode::Equals), Control::ZoomIn),
			(KeyMapping::Keyboard(VirtualKeyCode::Minus), Control::ZoomOut),
			(KeyMapping::Keyboard(VirtualKeyCode::Key0), Control::ZoomReset)
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
            match *key
            {
                ControlState::Clicked =>
                {
                    *key = ControlState::Held;
                },
                _ => ()
            }
        });
    }
}
