use yanyaengine::KeyCode;

use crate::client::game_state::{ControlState, KeyMapping, UiControls};

pub use core::*;

mod core;


pub fn text_input_handle<Id: Idable>(
    controls: &mut UiControls<Id>,
    position: &mut u32,
    text: &mut String
)
{
    let shift_left = |position: &mut u32|
    {
        *position = position.saturating_sub(1);
    };

    let shift_right_many = |text: &String, position: &mut u32, amount: u32|
    {
        *position = (*position + amount).min(text.chars().count() as u32);
    };

    let shift_right = |text: &String, position: &mut u32|
    {
        shift_right_many(text, position, 1);
    };

    controls.controls.retain(|(changed, state)|
    {
        if let KeyMapping::Keyboard(KeyCode::ControlLeft) = changed.key
        {
            controls.state.ctrl_held = state.is_down();
            return false;
        }

        if let (Some(logical), ControlState::Pressed) = (&changed.logical, state)
        {
            let current_index = text.char_indices().nth(*position as usize).map(|(index, _)| index).unwrap_or(text.len());

            if let KeyMapping::Keyboard(key) = changed.key
            {
                match key
                {
                    KeyCode::ArrowLeft =>
                    {
                        shift_left(position);
                        return false;
                    },
                    KeyCode::ArrowRight =>
                    {
                        shift_right(text, position);
                        return false;
                    },
                    KeyCode::Tab => return false,
                    KeyCode::Backspace =>
                    {
                        if !text.is_empty()
                        {
                            text.remove(current_index.saturating_sub(1));
                            shift_left(position);
                        }

                        return false;
                    },
                    KeyCode::KeyV if controls.state.ctrl_held =>
                    {
                        match controls.clipboard.get_text()
                        {
                            Ok(content) =>
                            {
                                text.insert_str(current_index, &content);
                                shift_right_many(text, position, content.chars().count() as u32);
                            },
                            Err(err) => eprintln!("error pasting from clipboard: {err}")
                        }

                        return false;
                    },
                    _ => ()
                }
            }

            if let Some(c) = logical.to_text()
            {
                text.insert_str(current_index, c);

                shift_right_many(text, position, c.chars().count() as u32);

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
