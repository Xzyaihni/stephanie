use crate::common::{
    render_info::*,
    Entity,
    ClientEntityInfo,
    entity::ClientEntities
};


pub struct EntityCreator<'a>
{
    pub entities: &'a mut ClientEntities
}

impl EntityCreator<'_>
{
    pub fn push(
        &mut self,
        info: ClientEntityInfo,
        render: RenderInfo
    ) -> Entity
    {
        let entity = self.entities.push_client_eager(info);

        self.entities.set_deferred_render(entity, render);

        entity
    }
}
