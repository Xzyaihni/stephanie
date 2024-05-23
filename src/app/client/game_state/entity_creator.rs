use crate::common::{
    Entity,
    RenderInfo,
    ClientEntityInfo,
    entity::ClientEntities
};


pub struct EntityCreator<'a>
{
    pub entities: &'a mut ClientEntities,
    pub objects: &'a mut Vec<(Entity, RenderInfo)>
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
}
