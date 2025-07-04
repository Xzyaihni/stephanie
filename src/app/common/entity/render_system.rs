use vulkano::descriptor_set::WriteDescriptorSet;

use yanyaengine::{game_object::*, SolidObject};

use crate::{
    debug_config::*,
    ProgramShaders,
    client::{Ui, VisibilityChecker},
    common::{
        render_info::*,
        Entity,
        world::World,
        entity::ClientEntities
    }
};


pub struct DrawEntities<'a>
{
    pub solid: &'a SolidObject,
    pub renders: &'a [Vec<Entity>],
    pub above_world: &'a [Entity],
    pub shaded_renders: &'a [Vec<Entity>],
    pub light_renders: &'a [Entity],
    pub world: &'a World
}

pub fn draw(
    entities: &ClientEntities,
    shaders: &ProgramShaders,
    ui: &Ui,
    renderables: DrawEntities,
    visibility: &VisibilityChecker,
    info: &mut DrawInfo,
    animation: f32
)
{
    info.bind_pipeline(shaders.world);

    renderables.world.draw_tiles(info, false);

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

    info.next_subpass();

    info.bind_pipeline(shaders.world_shaded);

    renderables.world.draw_tiles(info, true);

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

    info.next_subpass();

    info.bind_pipeline(shaders.sky_shadow);

    renderables.world.draw_sky_occluders(info);

    if DebugConfig::is_disabled(DebugTool::NoLighting)
    {
        let mut is_light_pipeline = false;
        let lights_len = renderables.light_renders.len();
        renderables.light_renders.iter().copied().enumerate().for_each(|(index, entity)|
        {
            let is_last = (index + 1) == lights_len;

            let light = entities.light(entity).unwrap();

            let mut has_shadows = false;
            renderables.world.draw_light_shadows(info, &light.visibility_checker(), index, |info|
            {
                info.bind_pipeline(shaders.light_shadow);
                is_light_pipeline = false;

                has_shadows = true;
            });

            if !is_light_pipeline
            {
                info.bind_pipeline(shaders.lighting);
                is_light_pipeline = true;
            }

            light.draw(info);

            if !is_last && has_shadows
            {
                info.bind_pipeline(shaders.clear_alpha);
                is_light_pipeline = false;

                renderables.solid.draw(info);
            }
        });
    }

    info.bind_pipeline(shaders.shadow);

    renderables.world.draw_shadows(info);

    if DebugConfig::is_disabled(DebugTool::NoOcclusion)
    {
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
    }

    info.next_subpass();
    info.bind_pipeline(shaders.final_mix);
    info.current_sets = vec![info.create_descriptor_set(0, [
        WriteDescriptorSet::image_view(0, info.attachments[0].clone()),
        WriteDescriptorSet::image_view(1, info.attachments[2].clone()),
        WriteDescriptorSet::image_view(2, info.attachments[4].clone())
    ])];

    renderables.solid.draw(info);

    info.next_subpass();

    info.current_sets.clear();
    info.bind_pipeline(shaders.above_world);

    renderables.above_world.iter().for_each(|&entity|
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

    info.bind_pipeline(shaders.ui);

    ui.draw(info);
}
