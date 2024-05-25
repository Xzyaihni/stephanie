use crate::common::{
    Entity,
    RenderInfo,
    RenderObject,
    ClientEntityInfo,
    entity::ClientEntities
};


pub struct EntityCreator<'a>
{
    pub entities: &'a mut ClientEntities,
    pub objects: &'a mut Vec<(Entity, RenderInfo)>,
    pub replace_objects: &'a mut Vec<(Entity, RenderObject)>
}

impl EntityCreator<'_>
{
    pub fn push(
        &mut self,
        info: ClientEntityInfo,
        render: RenderInfo
    ) -> Entity
    {
        let entity = self.entities.push(info);

        self.objects.push((entity, render));

        entity
    }

    pub fn replace(
        &mut self,
        entity: Entity,
        new_object: RenderObject
    )
    {
        self.replace_objects.push((entity, new_object));
    }
}
