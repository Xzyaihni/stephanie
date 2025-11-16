use vulkano::{
    descriptor_set::WriteDescriptorSet,
    buffer::BufferContents,
};

use nalgebra::{vector, Vector2};

use yanyaengine::{game_object::*, UniformLocation, SolidObject, Object};

use crate::{
    debug_config::*,
    app::{ProgramShaders, TimestampQuery},
    client::{Ui, game_state::ui::controller::UiShaders},
    common::{
        ENTITY_SCALE,
        ENTITY_PIXEL_SCALE,
        render_info::*,
        Side1d,
        Entity,
        AnyEntities,
        character::SpriteState,
        characters_info::FacialExpression,
        world::World,
        entity::ClientEntities,
        anatomy::{AnatomyId, OrganId}
    }
};


#[derive(BufferContents)]
#[repr(C)]
pub struct BackgroundColor
{
    pub color: [f32; 3]
}

#[derive(BufferContents)]
#[repr(C)]
pub struct MouseInfo
{
    pub amount: f32
}

pub struct DrawEntities<'a>
{
    pub solid: &'a SolidObject,
    pub mouse_solid: &'a Object,
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
    pub timestamp_query: TimestampQuery,
    pub is_loading: bool,
    pub cooldown_fraction: f32
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
            mouse_solid,
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
        timestamp_query,
        is_loading,
        cooldown_fraction
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

    if !is_loading
    {
        info.bind_pipeline(shaders.world);

        world.draw_tiles(info, false);
    }

    timing_end!(1);

    {
        let mut current = shaders.world;
        let mut try_bind = |info: &mut DrawInfo, shader|
        {
            if current != shader
            {
                info.bind_pipeline(shader);
                current = shader;
            }
        };

        let characters_info = &entities.infos().characters_info;

        renders.iter().flatten().for_each(|&entity|
        {
            let render = entities.render(entity).unwrap();

            let outline = OutlinedInfo::new(
                render.mix,
                render.outlined,
                animation
            );

            if let Some(character) = entities.character(entity)
            {
                if let SpriteState::Crawling = character.sprite_state()
                {
                    try_bind(info, shaders.default);

                    render.draw(info, outline);
                    return;
                }

                try_bind(info, shaders.character);

                let character_info = characters_info.get(character.id);

                let eyes_closed = character_info.face.eyes_closed;
                let eyes_normal = character_info.face.eyes_normal;

                let aspect = character_info.normal.scale.component_div(&character.sprite_texture(character_info).scale);

                let offset_value = |value: Vector2<f32>| -> Vector2<f32>
                {
                    ((value / ENTITY_PIXEL_SCALE as f32) * ENTITY_SCALE)
                        .component_div(&character_info.normal.scale)
                };

                let face_offset: [f32; 2] = if let SpriteState::Lying = character.sprite_state()
                {
                    let pixel_offset = character_info.lying_face_offset;

                    offset_value(pixel_offset.cast()).into()
                } else
                {
                    [0.0; 2]
                };

                let eyes_offset = if let Some(enemy) = entities.enemy(entity)
                {
                    if enemy.seen_fraction().is_some()
                    {
                        let x = (animation * 2.0 - 1.0).abs() * -2.0 + 1.0;

                        let offset = (if x < 0.0 { x * -x } else { x * x }) * 3.0;

                        offset_value(vector![0.0, offset]).into()
                    } else
                    {
                        [0.0, 0.0]
                    }
                } else
                {
                    [0.0, 0.0]
                };

                let aspect: [f32; 2] = aspect.into();

                let shader_info = entities.anatomy(entity).map(|anatomy|
                {
                    let face = character.facial_expression(&anatomy);

                    let draw_eyes = match face
                    {
                        FacialExpression::Normal | FacialExpression::Sick => true,
                        _ => false
                    };

                    let is_eye_closed = |side|
                    {
                        (draw_eyes
                            && anatomy.get_human::<()>(AnatomyId::Organ(OrganId::Eye(side))).unwrap().is_none())
                            || !anatomy.is_conscious()
                    };

                    let left_closed = is_eye_closed(Side1d::Left);
                    let right_closed = is_eye_closed(Side1d::Right);

                    let face_textures = &character_info.face;
                    let face = match face
                    {
                        FacialExpression::Normal => face_textures.normal,
                        FacialExpression::Hurt => face_textures.hurt,
                        FacialExpression::Sick => face_textures.sick,
                        FacialExpression::Dead => face_textures.dead
                    };

                    CharacterShaderInfo{
                        draw_eyes,
                        left_closed,
                        right_closed,
                        face,
                        eyes_closed,
                        eyes_normal,
                        aspect,
                        face_offset,
                        eyes_offset
                    }
                }).unwrap_or_else(||
                {
                    CharacterShaderInfo{
                        draw_eyes: true,
                        left_closed: false,
                        right_closed: false,
                        face: character_info.face.normal,
                        eyes_closed,
                        eyes_normal,
                        aspect,
                        face_offset,
                        eyes_offset
                    }
                });

                {
                    let assets = info.object_info.assets.lock();

                    let set_of = |id, set|
                    {
                        assets.texture(id).lock().descriptor_set(info, UniformLocation{set, binding: 0})
                    };

                    info.current_sets = vec![
                        set_of(shader_info.face, 0),
                        set_of(shader_info.eyes_closed, 1),
                        set_of(shader_info.eyes_normal, 2)
                    ];
                }

                let shader_info = CharacterShaderInfoRaw::new(outline, shader_info);

                render.draw(info, shader_info);
                info.current_sets.clear();
            } else
            {
                try_bind(info, shaders.default);

                render.draw(info, outline);
            }
        });
    }

    info.next_subpass();

    timing_end!(2);

    if !is_loading
    {
        info.bind_pipeline(shaders.world_shaded);

        world.draw_tiles(info, true);
    }

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

    if !is_loading
    {
        info.bind_pipeline(shaders.sky_shadow);

        world.draw_sky_occluders(info);
    }

    timing_end!(4);

    if !is_loading
    {
        info.bind_pipeline(shaders.sky_lighting);

        info.push_constants(BackgroundColor{color: light_color});

        world.draw_sky_lights(info);
    }

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

    if !is_loading
    {
        info.bind_pipeline(shaders.shadow);

        world.draw_shadows(info);
    }

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
        let render = entities.render(entity).unwrap();

        let outline = OutlinedInfo::new(
            render.mix,
            render.outlined,
            animation
        );

        render.draw(info, outline);
    });

    info.bind_pipeline(shaders.ui);

    ui.draw(info, &UiShaders{ui: shaders.ui, ui_fill: shaders.ui_fill});

    info.bind_pipeline(shaders.mouse);

    if !is_loading && cooldown_fraction > 0.0
    {
        info.push_constants(MouseInfo{amount: cooldown_fraction});

        mouse_solid.draw(info);
    }

    timing_end!(8);
}
