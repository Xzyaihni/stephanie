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
    pub entities: &'a mut ClientEntities,
    pub objects: &'a mut Vec<(bool, Entity, ReplaceObject)>
}

impl EntityCreator<'_>
{
    pub fn push(
        &mut self,
        info: ClientEntityInfo,
        render: RenderInfo
    ) -> Entity
    {
        let entity = self.entities.push(true, info);

        self.objects.push((true, entity, ReplaceObject::Full(render)));

        entity
    }

    pub fn replace_object(
        &mut self,
        entity: Entity,
        object: RenderObject
    )
    {
        self.objects.push((true, entity, ReplaceObject::Object(object)));
    }

    pub fn replace_scissor(
        &mut self,
        entity: Entity,
        scissor: Scissor
    )
    {
        self.objects.push((true, entity, ReplaceObject::Scissor(scissor)));
    }
}
