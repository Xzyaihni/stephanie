use nalgebra::Vector3;

use yanyaengine::game_object::*;

use crate::{
    debug_config::*,
    ProgramShaders,
    client::VisibilityChecker,
    common::{
        render_info::*,
        Entity,
        MixColor,
        OccludingCaster,
        world::World,
        entity::ClientEntities
    }
};


pub fn update_buffers(
    entities: &ClientEntities,
    renderables: impl Iterator<Item=Entity>,
    lights: impl Iterator<Item=Entity>,
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
                    let color = if physical.sleeping()
                    {
                        [0.2, 0.2, 1.0, 1.0]
                    } else
                    {
                        [0.2, 1.0, 0.2, 1.0]
                    };

                    render.mix = Some(MixColor{color, amount: 0.7, keep_transparency: true});
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

    lights.for_each(|entity|
    {
        let position = entities.transform(entity).map(|x| x.position).unwrap_or_else(Vector3::zeros);
        let mut light = entities.light_mut(entity).unwrap();

        light.update_buffers(info, position);
    });
}

pub struct DrawEntities<'a>
{
    pub renders: &'a [Vec<Entity>],
    pub shaded_renders: &'a [Vec<Entity>],
    pub light_renders: &'a [Vec<Entity>],
    pub world: &'a World
}

pub fn draw(
    entities: &ClientEntities,
    shaders: &ProgramShaders,
    renderables: DrawEntities,
    visibility: &VisibilityChecker,
    info: &mut DrawInfo,
    animation: f32
)
{
    info.bind_pipeline(shaders.shadow);

    renderables.world.draw_shadows(info, visibility);

    renderables.renders.iter().flatten().copied().filter_map(|entity|
    {
        entities.occluder(entity)
    }).for_each(|occluder|
    {
        if !occluder.visible(visibility)
        {
            return;
        }

        occluder.draw(info);
    });

    info.bind_pipeline(shaders.world);

    renderables.world.draw(info);

    info.bind_pipeline(shaders.default);

    renderables.renders.iter().flatten().for_each(|&entity|
    {
        let outline = entities.outlineable(entity).and_then(|outline|
        {
            outline.current()
        }).unwrap_or_default();

        let render = entities.render(entity).unwrap();

        let outline = OutlinedInfo::new(
            render.mix,
            outline,
            animation
        );

        render.draw(info, outline);
    });

    info.bind_pipeline(shaders.world_shaded);

    renderables.world.draw(info);

    info.bind_pipeline(shaders.default_shaded);

    renderables.light_renders.iter().flatten().copied().for_each(|entity|
    {
        entities.light(entity).unwrap().draw(info);
    });

    info.bind_pipeline(shaders.default_shaded);

    renderables.shaded_renders.iter().flatten().copied().for_each(|entity|
    {
        let render = entities.render(entity).unwrap();

        render.draw(info, OutlinedInfo::new(
            render.mix,
            Default::default(),
            animation
        ));
    });
}
