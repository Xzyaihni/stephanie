use crate::common::{
    render_info::*,
    Entity,
    ClientEntityInfo,
    entity::ClientEntities
};


pub enum ReplaceObject
{
    Full(RenderInfo),
    Object(RenderObject),
    Scissor(Scissor)
}

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
        let entity = self.entities.push_client(true, info);

        self.entities.set_deferred_render(entity, render);

        entity
    }

    pub fn replace_scissor(
        &mut self,
        entity: Entity,
        scissor: Scissor
    )
    {
        todo!();
    }
}
