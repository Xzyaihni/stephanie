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
                    let color = if physical.sleeping()
                    {
                        [0.2, 0.2, 1.0]
                    } else
                    {
                        [0.2, 1.0, 0.2]
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
}

pub struct DrawEntities<'a, I>
{
    pub renders: &'a [Entity],
    pub ui_renders: I
}

pub fn draw<I: Iterator<Item=Entity>>(
    entities: &ClientEntities,
    shaders: &ProgramShaders,
    renderables: DrawEntities<I>,
    visibility: &VisibilityChecker,
    info: &mut DrawInfo,
    animation: f32
)
{
    renderables.renders.iter().for_each(|&entity|
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

    info.set_depth_test(false);

    info.bind_pipeline(shaders.shadow);
    renderables.renders.iter().filter_map(|entity|
    {
        entities.occluder(*entity)
    }).for_each(|occluder|
    {
        if !occluder.visible(visibility)
        {
            return;
        }

        occluder.draw(info);
    });

    info.bind_pipeline(shaders.ui);

    renderables.ui_renders.for_each(|entity|
    {
        let render = entities.render(entity).unwrap();

        let outline = UiOutlinedInfo::new(render.mix);

        render.draw(info, outline);
    });

    info.set_depth_test(true);
}
