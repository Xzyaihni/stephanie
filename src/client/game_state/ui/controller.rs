use yanyaengine::KeyCode;

use crate::client::game_state::{ControlState, KeyMapping, UiControls};

pub use core::*;

mod core;

pub fn text_input_handle<Id: Idable>(
    controls: &mut UiControls<Id>,
    text: &mut String
)
{
    controls.controls.retain(|((key, logical), state)|
    {
        if let KeyMapping::Keyboard(KeyCode::ControlLeft) = key
        {
            controls.state.ctrl_held = state.is_down();
            return false;
        }

        if let (Some(logical), ControlState::Pressed) = (logical, state)
        {
            if let KeyMapping::Keyboard(key) = key
            {
                match key
                {
                    KeyCode::Tab => return false,
                    KeyCode::Backspace =>
                    {
                        text.pop();
                        return false;
                    },
                    KeyCode::KeyV if controls.state.ctrl_held =>
                    {
                        match controls.clipboard.get_text()
                        {
                            Ok(content) => *text += &content,
                            Err(err) => eprintln!("error pasting from clipboard: {err}")
                        }

                        return false;
                    },
                    _ => ()
                }
            }

            if let Some(c) = logical.to_text()
            {
                *text += c;

                return false;
            }
        }

        true
    });
}

pub fn scrollbar_handle<Id: Idable>(
    controls: &mut UiControls<Id>,
    scrollbar: TreeInserter<Id>,
    scrollbar_id: &Id,
    bar_size: f32,
    horizontal: bool,
    taken: bool
) -> Option<f32>
{
    if let Some(position) = scrollbar.mouse_position_mapped()
    {
        let position = if horizontal { position.x } else { position.y };

        if scrollbar.is_mouse_inside() && !taken
        {
            controls.poll_action_held(scrollbar_id);
        }

        if controls.observe_action_held(scrollbar_id)
        {
            let value = if bar_size > 0.99
            {
                0.0
            } else
            {
                let half_bar_size = bar_size / 2.0;
                (position.clamp(half_bar_size, 1.0 - half_bar_size) - half_bar_size) / (1.0 - bar_size)
            };

            return Some(value);
        }
    }

    None
}
