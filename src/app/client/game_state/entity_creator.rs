use yanyaengine::game_object::*;

use crate::common::{
    ServerToClient,
    Entity,
    RenderInfo,
    ClientRenderInfo,
    RenderObject,
    ClientEntityInfo,
    render_info::ClientRenderObject,
    entity::ClientEntities
};


pub struct EntityCreator<'a, 'b>
{
    pub object_info: &'a mut ObjectCreateInfo<'b>,
    pub entities: &'a mut ClientEntities
}

impl EntityCreator<'_, '_>
{
    pub fn to_client(&mut self, render: RenderInfo) -> ClientRenderInfo
    {
        render.server_to_client(Some(Default::default()), self.object_info)
    }

    pub fn to_client_object(&mut self, object: RenderObject) -> Option<ClientRenderObject>
    {
        object.into_client(Default::default(), self.object_info)
    }

    pub fn push(&mut self, info: ClientEntityInfo) -> Entity
    {
        self.entities.push(info)
    }
}
