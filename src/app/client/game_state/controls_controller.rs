use std::{
    mem,
    error,
    fmt::{self, Display},
    collections::HashMap
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

    pub fn is_down(self) -> bool
    {
        self.to_bool()
    }

    pub fn is_up(self) -> bool
    {
        !self.is_down()
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

struct ControlsState<Id>
{
    is_click_held: Option<Id>,
    click_mappings: Vec<KeyMapping>
}

pub struct UiControls<Id>
{
    click_taken: bool,
    state: ControlsState<Id>,
    controls: HashMap<KeyMapping, ControlState>
}

impl<Id: PartialEq + Clone> UiControls<Id>
{
    pub fn take_click_down(&mut self) -> bool
    {
        self.state.click_mappings.iter().fold(false, |acc, mapping|
        {
            let is_down = self.controls.get(mapping).map(|x| x.is_down()).unwrap_or(false);

            if is_down
            {
                self.click_taken = true;
                self.controls.remove(mapping);
            }

            acc || is_down
        })
    }

    pub fn is_click_taken(&self) -> bool
    {
        self.click_taken
    }

    pub fn is_click_down(&self) -> bool
    {
        self.state.click_mappings.iter().any(|mapping|
        {
            self.controls.get(mapping).map(|x| x.is_down()).unwrap_or(false)
        })
    }

    pub fn poll_action_held(&mut self, id: &Id) -> bool
    {
        if self.take_click_down()
        {
            self.state.is_click_held = Some(id.clone());
        }

        self.observe_action_held(id)
    }

    pub fn observe_action_held(&self, id: &Id) -> bool
    {
        self.state.is_click_held.as_ref().map(|x| x == id).unwrap_or(false)
    }
}

pub struct ControlsController<Id>
{
    clipboard: Option<ClipboardContext>,
    controls_state: Option<ControlsState<Id>>,
    key_mapping: BiMap<KeyMapping, Control>,
    keys: [ControlState; Control::COUNT],
    changed: HashMap<KeyMapping, ControlState>
}

impl<Id> ControlsController<Id>
{
    pub fn new() -> Self
    {
        let key_mapping: BiMap<KeyMapping, Control> = [
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

        let click_mappings = key_mapping.iter().filter_map(|(mapping, control)|
        {
            if let Control::MainAction = control
            {
                Some(*mapping)
            } else
            {
                None
            }
        }).collect();

        let controls_state = Some(ControlsState{
            is_click_held: None,
            click_mappings
        });

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
            controls_state,
            key_mapping,
            keys: [ControlState::Released; Control::COUNT],
            changed: HashMap::new()
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

    pub fn handle_input(&mut self, input: yanyaengine::Control) -> bool
    {
        let state = ControlState::from(&input);

        let this_key = KeyMapping::from_control(input);

        if let Some(this_key) = this_key
        {
            self.changed.insert(this_key, state);

            true
        } else
        {
            false
        }
    }

    pub fn key_for(&self, control: &Control) -> Option<&KeyMapping>
    {
        self.key_mapping.get_back(control)
    }

    pub fn changed_this_frame(&mut self) -> UiControls<Id>
    {
        UiControls{
            click_taken: false,
            state: mem::take(&mut self.controls_state).unwrap(),
            controls: mem::take(&mut self.changed)
        }
    }

    pub fn consume_changed(
        &mut self,
        changed: UiControls<Id>
    ) -> impl Iterator<Item=(Control, ControlState)> + use<'_, Id>
    {
        self.controls_state = Some(changed.state);
        changed.controls.into_iter().filter_map(|(key, state)|
        {
            self.key_mapping.get(&key).cloned().map(|matched|
            {
                self.keys[matched as usize] = state;
                if let Control::MainAction = matched
                {
                    if state.is_up()
                    {
                        self.controls_state.as_mut().unwrap().is_click_held = None;
                    }
                }

                (matched, state)
            })
        })
    }
}
