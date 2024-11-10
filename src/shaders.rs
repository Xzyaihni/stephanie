use vulkano::pipeline::graphics::depth_stencil::{
    DepthState,
    StencilState,
    StencilOpState,
    StencilOps,
    StencilOp,
    CompareOp
};

use nalgebra::Vector3;

use yanyaengine::{ShadersContainer, Shader, ShadersGroup, ShadersQuery};

use crate::{
    BACKGROUND_COLOR,
    app::ProgramShaders,
    common::world::TILE_SIZE
};


mod default_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/default.vert"
    }
}

mod default_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/default.frag"
    }
}

mod world_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/world.frag"
    }
}

mod default_shaded_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/default_shaded.frag"
    }
}

mod world_shaded_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/world_shaded.vert"
    }
}

mod world_shaded_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/world_shaded.frag"
    }
}

mod shadow_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/shadow.vert"
    }
}

mod shadow_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/shadow.frag"
    }
}

mod ui_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/ui.vert"
    }
}

mod ui_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/ui.frag"
    }
}


const DARKEN: f32 = 0.97;
const SHADOW_COLOR: Vector3<f32> = Vector3::new(0.07, 0.02, 0.1);

pub struct ShadersCreated
{
    pub shaders: ShadersContainer,
    pub group: ProgramShaders,
    pub query: ShadersQuery
}

pub fn create() -> ShadersCreated
{
    let mut shaders = ShadersContainer::new();

    let default_vertex = |device|
    {
        default_vertex::load(device).unwrap().specialize(
            [(0, TILE_SIZE.into())].into_iter().collect()
        )
    };

    let create_stencil = |stencil| StencilState{front: stencil, back: stencil};
    let default_stencil = create_stencil(StencilOpState{
        ops: StencilOps{
            compare_op: CompareOp::Equal,
            ..Default::default()
        },
        reference: 1,
        ..Default::default()
    });

    let world_depth = DepthState{
        write_enable: true,
        compare_op: CompareOp::Always
    };

    let default_shader = shaders.push(Shader{
        shader: ShadersGroup::new(
            default_vertex,
            default_fragment::load
        ),
        stencil: Some(default_stencil.clone()),
        depth: Some(DepthState{
            write_enable: false,
            compare_op: CompareOp::Less
        }),
        ..Default::default()
    });

    let world_shader = shaders.push(Shader{
        shader: ShadersGroup::new(
            default_vertex,
            world_fragment::load
        ),
        stencil: Some(default_stencil),
        depth: Some(world_depth),
        ..Default::default()
    });

    let shaded_stencil = create_stencil(StencilOpState{
        ops: StencilOps{
            compare_op: CompareOp::Equal,
            ..Default::default()
        },
        reference: 0,
        ..Default::default()
    });

    let shaded_specialization = [
        (0, DARKEN.into()),
        (1, SHADOW_COLOR.x.into()),
        (2, SHADOW_COLOR.y.into()),
        (3, SHADOW_COLOR.z.into())
    ];

    let world_shaded_shader = {
        let shaded_specialization = shaded_specialization.clone();
        shaders.push(Shader{
            shader: ShadersGroup::new(
                move |device|
                {
                    world_shaded_vertex::load(device).unwrap().specialize(
                        [(0, TILE_SIZE.into())].into_iter().collect()
                    )
                },
                move |device|
                {
                    world_shaded_fragment::load(device).unwrap().specialize(
                        shaded_specialization.into_iter().collect()
                    )
                }
            ),
            stencil: Some(shaded_stencil.clone()),
            depth: Some(world_depth),
            ..Default::default()
        })
    };

    let default_shaded_shader = shaders.push(Shader{
        shader: ShadersGroup::new(
            default_vertex,
            move |device|
            {
                default_shaded_fragment::load(device).unwrap().specialize(
                    shaded_specialization.into_iter().collect()
                )
            }
        ),
        stencil: Some(shaded_stencil),
        depth: Some(DepthState::simple()),
        ..Default::default()
    });

    let shadow_color = BACKGROUND_COLOR.lerp(&SHADOW_COLOR, DARKEN);

    let shadow_shader = shaders.push(Shader{
        shader: ShadersGroup::new(
            shadow_vertex::load,
            move |device|
            {
                shadow_fragment::load(device).unwrap().specialize(
                    [
                        (0, shadow_color.x.into()),
                        (1, shadow_color.y.into()),
                        (2, shadow_color.z.into())
                    ].into_iter().collect()
                )
            }
        ),
        stencil: Some(create_stencil(StencilOpState{
            ops: StencilOps{
                pass_op: StencilOp::Zero,
                compare_op: CompareOp::Always,
                ..Default::default()
            },
            ..Default::default()
        })),
        ..Default::default()
    });

    let ui_shader = shaders.push(Shader{
        shader: ShadersGroup::new(
            ui_vertex::load,
            ui_fragment::load
        ),
        ..Default::default()
    });

    ShadersCreated{
        shaders,
        group: ProgramShaders{
            default: default_shader,
            default_shaded: default_shaded_shader,
            world: world_shader,
            world_shaded: world_shaded_shader,
            shadow: shadow_shader,
            ui: ui_shader
        },
        query: Box::new(move |path|
        {
            if path.starts_with("ui")
            {
                ui_shader
            } else
            {
                default_shader
            }
        })
    }
}
