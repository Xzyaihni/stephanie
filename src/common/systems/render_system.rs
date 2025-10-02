use vulkano::{
    descriptor_set::WriteDescriptorSet,
    buffer::BufferContents,
};

use yanyaengine::{game_object::*, SolidObject};

use crate::{
    debug_config::*,
    app::{ProgramShaders, TimestampQuery},
    client::Ui,
    common::{
        render_info::*,
        Entity,
        world::World,
        entity::ClientEntities
    }
};


#[derive(BufferContents)]
#[repr(C)]
pub struct BackgroundColor
{
    pub color: [f32; 3]
}

pub struct DrawEntities<'a>
{
    pub solid: &'a SolidObject,
    pub renders: &'a [Vec<Entity>],
    pub above_world: &'a [Entity],
    pub occluders: &'a [Entity],
    pub shaded_renders: &'a [Vec<Entity>],
    pub light_renders: &'a [Entity],
    pub world: &'a World
}

pub struct DrawingInfo<'a, 'b, 'c>
{
    pub shaders: &'a ProgramShaders,
    pub info: &'b mut DrawInfo<'c>,
    pub timestamp_query: TimestampQuery
}

pub struct SkyColors
{
    pub light_color: [f32; 3]
}

pub fn draw(
    entities: &ClientEntities,
    ui: &Ui,
    DrawEntities{
        solid,
        renders,
        above_world,
        occluders,
        shaded_renders,
        light_renders,
        world
    }: DrawEntities,
    DrawingInfo{
        shaders,
        info,
        timestamp_query
    }: DrawingInfo,
    SkyColors{
        light_color
    }: SkyColors,
    animation: f32
)
{
    macro_rules! timing_start
    {
        ($index:literal) =>
        {
            if DebugConfig::is_enabled(DebugTool::GpuDrawTimings)
            {
                timestamp_query.start(info, $index);
            }
        }
    }

    macro_rules! timing_end
    {
        ($index:literal) =>
        {
            if DebugConfig::is_enabled(DebugTool::GpuDrawTimings)
            {
                timestamp_query.end(info, $index);
            }
        }
    }

    timing_start!(0);

    info.bind_pipeline(shaders.world);

    world.draw_tiles(info, false);

    timing_end!(1);

    info.bind_pipeline(shaders.default);

    renders.iter().flatten().for_each(|&entity|
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

    timing_end!(2);

    info.bind_pipeline(shaders.world_shaded);

    world.draw_tiles(info, true);

    info.bind_pipeline(shaders.default_shaded);

    shaded_renders.iter().flatten().copied().for_each(|entity|
    {
        let render = entities.render(entity).unwrap();

        render.draw(info, OutlinedInfo::new(
            render.mix,
            Default::default(),
            animation
        ));
    });

    info.next_subpass();

    timing_end!(3);

    info.bind_pipeline(shaders.sky_shadow);

    world.draw_sky_occluders(info);

    timing_end!(4);

    info.bind_pipeline(shaders.sky_lighting);

    info.push_constants(BackgroundColor{color: light_color});

    world.draw_sky_lights(info);

    timing_end!(5);

    if DebugConfig::is_disabled(DebugTool::NoLighting)
    {
        light_renders.iter().copied().enumerate().for_each(|(index, entity)|
        {
            let light = entities.light(entity).unwrap();

            if index != 0
            {
                info.bind_pipeline(shaders.clear_alpha);

                light.draw(info);
            }

            world.draw_light_shadows(info, &light.visibility_checker(), index, |info|
            {
                info.bind_pipeline(shaders.light_shadow);
            });

            info.bind_pipeline(shaders.lighting);
            light.draw(info);
        });
    }

    timing_end!(6);

    info.bind_pipeline(shaders.shadow);

    world.draw_shadows(info);

    timing_end!(7);

    if DebugConfig::is_disabled(DebugTool::NoOcclusion)
    {
        occluders.iter().copied().for_each(|entity|
        {
            entities.occluder(entity).unwrap().draw(info);
        });
    }

    info.next_subpass();
    info.bind_pipeline(shaders.final_mix);
    info.current_sets = vec![info.create_descriptor_set(0, [
        WriteDescriptorSet::image_view(0, info.attachments[0].clone()),
        WriteDescriptorSet::image_view(1, info.attachments[2].clone()),
        WriteDescriptorSet::image_view(2, info.attachments[4].clone())
    ])];

    solid.draw(info);

    info.next_subpass();

    info.current_sets.clear();
    info.bind_pipeline(shaders.above_world);

    above_world.iter().for_each(|&entity|
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

    timing_end!(8);
}
