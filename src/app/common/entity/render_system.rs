use yanyaengine::game_object::*;

use crate::{
    debug_config::*,
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
    caster: &OccludingCaster
)
{
    renderables.for_each(|entity|
    {
        if DebugConfig::is_enabled(DebugTool::Sleeping)
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

        let transform = entities.transform(entity).unwrap();

        let mut render = entities.render_mut(entity).unwrap();
        render.set_transform(transform.clone());
        render.update_buffers(info);

        if let Some(mut occluder) = entities.occluder_mut(entity)
        {
            occluder.set_transform(transform.clone());
            occluder.update_buffers(info, caster);
        }
    });
}
