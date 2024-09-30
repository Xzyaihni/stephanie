use yanyaengine::game_object::*;

use crate::{
    DEBUG_SLEEPING,
    common::{
        Entity,
        MixColor,
        OccludingCaster,
        entity::ClientEntities
    }
};


pub fn update_buffers(
    entities: &ClientEntities,
    renderables: impl Iterator<Item=Entity>,
    info: &mut UpdateBuffersInfo,
    casters: &OccludingCaster
)
{
    renderables.for_each(|entity|
    {
        let transform = entities.transform(entity).unwrap().clone();

        if DEBUG_SLEEPING
        {
            if let Some(physical) = entities.physical(entity)
            {
                if let Some(mut render) = entities.render_mut(entity)
                {
                    render.mix = Some(if physical.sleeping()
                    {
                        MixColor{color: [0.2, 0.2, 1.0], amount: 0.7}
                    } else
                    {
                        MixColor{color: [0.2, 1.0, 0.2], amount: 0.7}
                    });
                }
            }
        }

        if let Some(mut render) = entities.render_mut(entity)
        {
            render.set_transform(transform);
            render.update_buffers(info);
        } else if let Some(mut occluding_plane) = entities.occluding_plane_mut(entity)
        {
            occluding_plane.set_transform(transform);
            occluding_plane.update_buffers(info, casters);
        } else
        {
            unreachable!();
        }
    });
}
