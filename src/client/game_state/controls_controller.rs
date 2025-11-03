use std::{
    mem,
    rc::{Rc, Weak},
    cell::RefCell,
    fmt::{self, Display}
};

use yanyaengine::{ElementState, PhysicalKey, Key, KeyCode, KeyCodeNamed, MouseButton};

use strum::EnumCount;

use arboard::Clipboard;

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
    Pause,
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
    pub fn from_control(value: yanyaengine::Control) -> Option<ChangedKey>
    {
        match value
        {
            yanyaengine::Control::Keyboard{
                keycode: PhysicalKey::Code(code),
                logical,
                repeat,
                ..
            } => Some(ChangedKey{key: KeyMapping::Keyboard(code), logical: Some(logical), repeat}),
            yanyaengine::Control::Mouse{button, ..} => Some(ChangedKey{key: KeyMapping::Mouse(button), logical: None, repeat: false}),
            yanyaengine::Control::Scroll{..} => None,
            _ => None
        }
    }
}

#[derive(Debug)]
pub struct ControlsState<Id>
{
    is_click_held: Option<Id>,
    pub ctrl_held: bool,
    click_mappings: Vec<KeyMapping>
}

#[derive(Debug)]
pub struct ClipboardWrapper(Weak<RefCell<Option<Clipboard>>>);

impl ClipboardWrapper
{
    pub fn get_text(&self) -> Result<String, ClipboardError>
    {
        let clipboard = self.0.upgrade().ok_or(ClipboardError::DoesntExist)?;

        let mut clipboard = clipboard.borrow_mut();

        clipboard.as_mut()
            .ok_or(ClipboardError::DoesntExist)
            .and_then(|clipboard| clipboard.get_text().map_err(ClipboardError::Clipboard))
    }
}

#[derive(Debug)]
pub struct UiControls<Id>
{
    pub clipboard: ClipboardWrapper,
    click_taken: bool,
    pub state: ControlsState<Id>,
    pub controls: Vec<(ChangedKey, ControlState)>
}

impl<Id: PartialEq + Clone> UiControls<Id>
{
    pub fn take_key_down(&mut self, key: KeyMapping) -> bool
    {
        let key_id = self.controls.iter().position(|(changed, _)| key == changed.key);
        let is_down = key_id.map(|index| self.controls[index].1.is_down()).unwrap_or(false);

        if is_down
        {
            self.controls.remove(key_id.unwrap());
        }

        is_down
    }

    pub fn take_click_down(&mut self) -> bool
    {
        self.state.click_mappings.iter().fold(false, |acc, mapping|
        {
            let key_id = self.controls.iter().position(|(changed, _)| *mapping == changed.key);
            let is_down = key_id.map(|index| self.controls[index].1.is_down()).unwrap_or(false);

            if is_down
            {
                self.click_taken = true;
                self.controls.remove(key_id.unwrap());
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
            self.controls.iter().find(|(changed, _)| *mapping == changed.key).map(|(_, x)| x.is_down()).unwrap_or(false)
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

pub enum ClipboardError
{
    DoesntExist,
    Clipboard(arboard::Error)
}

impl Display for ClipboardError
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::DoesntExist => write!(f, "clipboard doesnt exist"),
            Self::Clipboard(err) => Display::fmt(err, f)
        }
    }
}

#[derive(Debug)]
pub struct ChangedKey
{
    pub key: KeyMapping,
    pub logical: Option<Key>,
    pub repeat: bool
}

pub struct ControlsController<Id>
{
    clipboard: Rc<RefCell<Option<Clipboard>>>,
    controls_state: Option<ControlsState<Id>>,
    key_mapping: BiMap<KeyMapping, Control>,
    keys: [ControlState; Control::COUNT],
    changed: Vec<(ChangedKey, ControlState)>
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
            (KeyMapping::Keyboard(KeyCode::Escape), Control::Pause),
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
            ctrl_held: false,
            click_mappings
        });

        let clipboard = match Clipboard::new()
        {
            Ok(x) => Some(x),
            Err(err) =>
            {
                eprintln!("error getting clipboard: {err}");

                None
            }
        };

        let clipboard = Rc::new(RefCell::new(clipboard));

        Self{
            clipboard,
            controls_state,
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

    pub fn handle_input(&mut self, input: yanyaengine::Control) -> bool
    {
        let state = ControlState::from(&input);

        let this_key = KeyMapping::from_control(input);

        if let Some(this_key) = this_key
        {
            self.changed.push((this_key, state));

            true
        } else
        {
            false
        }
    }

    pub fn key_name(&self, control: &Control) -> String
    {
        self.key_for(control).map(ToString::to_string).unwrap_or_else(|| "unassigned".to_owned())
    }

    pub fn key_for(&self, control: &Control) -> Option<&KeyMapping>
    {
        self.key_mapping.get_back(control)
    }

    pub fn changed_this_frame(&mut self) -> UiControls<Id>
    {
        UiControls{
            clipboard: ClipboardWrapper(Rc::downgrade(&self.clipboard)),
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
        changed.controls.into_iter().filter_map(|(ChangedKey{key, repeat, ..}, state)|
        {
            if repeat
            {
                return None;
            }

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
