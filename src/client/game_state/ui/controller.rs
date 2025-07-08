use crate::client::game_state::UiControls;

pub use core::*;

mod core;

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
