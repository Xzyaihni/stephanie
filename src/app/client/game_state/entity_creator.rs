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

    pub fn replace(
        &mut self,
        entity: Entity,
        new_object: RenderObject
    )
    {
        if let Some(render) = self.entities.render(entity)
        {
            let new_render = RenderInfo{
                object: Some(new_object),
                shape: render.shape,
                z_level: render.z_level
            };

            self.objects.push((entity, new_render));
        }
    }
}
