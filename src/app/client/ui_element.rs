use yanyaengine::{Transform, game_object::*};

use crate::common::ServerToClient;


#[derive(Debug, Clone, Copy)]
pub enum UiElementType
{
    Panel,
    Button
}

#[derive(Debug, Clone)]
pub struct UiElement
{
    pub kind: UiElementType
}

impl ServerToClient<UiElement> for ()
{
    fn server_to_client(
        self,
        _transform: Option<Transform>,
        _create_info: &mut ObjectCreateInfo
    ) -> UiElement
    {
        unreachable!()
    }
}
