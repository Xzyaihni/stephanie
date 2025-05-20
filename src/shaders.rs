use vulkano::pipeline::graphics::{
    color_blend::{AttachmentBlend, BlendFactor, BlendOp},
    vertex_input::Vertex,
    depth_stencil::{
        DepthState,
        CompareOp
    }
};

use nalgebra::Vector3;

use yanyaengine::{
    OccludingPlane,
    Object,
    ObjectVertex,
    SimpleVertex,
    ShadersContainer,
    Shader,
    ShadersGroup
};

use crate::{
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

mod occluder_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/occluder.vert"
    }
}

mod occluder_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/occluder.frag"
    }
}

mod light_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/light.vert"
    }
}

mod light_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/light.frag"
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

mod final_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/final.vert"
    }
}

mod final_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/final.frag"
    }
}


const DARKEN: f32 = 0.97;
const SHADOW_COLOR: Vector3<f32> = Vector3::new(0.07, 0.02, 0.1);

pub struct ShadersCreated
{
    pub shaders: ShadersContainer,
    pub group: ProgramShaders
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

    let world_depth = DepthState{
        write_enable: true,
        compare_op: CompareOp::Always
    };

    let object_depth = DepthState{
        write_enable: false,
        compare_op: CompareOp::Less
    };

    let default_shader = shaders.push(Shader{
        shader: ShadersGroup::new(
            default_vertex,
            default_fragment::load
        ),
        depth: Some(object_depth),
        per_vertex: Some(Object::per_vertex()),
        subpass: 0,
        ..Default::default()
    });

    let world_shader = shaders.push(Shader{
        shader: ShadersGroup::new(
            default_vertex,
            world_fragment::load
        ),
        depth: Some(world_depth),
        per_vertex: Some(Object::per_vertex()),
        subpass: 0,
        ..Default::default()
    });

    let shaded_specialization = [
        (0, DARKEN.into()),
        (1, SHADOW_COLOR.x.into()),
        (2, SHADOW_COLOR.y.into()),
        (3, SHADOW_COLOR.z.into())
    ];

    let world_shaded_shader = shaders.push(Shader{
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
        depth: Some(world_depth),
        per_vertex: Some(Object::per_vertex()),
        subpass: 1,
        ..Default::default()
    });

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
        depth: Some(object_depth),
        per_vertex: Some(Object::per_vertex()),
        subpass: 1,
        ..Default::default()
    });

    let shadow_shader = shaders.push(Shader{
        shader: ShadersGroup::new(
            occluder_vertex::load,
            occluder_fragment::load
        ),
        depth: Some(DepthState{
            write_enable: false,
            compare_op: CompareOp::Always
        }),
        per_vertex: Some(OccludingPlane::per_vertex()),
        subpass: 2,
        blend: None,
        ..Default::default()
    });

    let occluder_shader = shaders.push(Shader{
        shader: ShadersGroup::new(
            occluder_vertex::load,
            occluder_fragment::load
        ),
        depth: Some(DepthState{
            write_enable: false,
            compare_op: CompareOp::Always
        }),
        per_vertex: Some(OccludingPlane::per_vertex()),
        subpass: 2,
        blend: None,
        ..Default::default()
    });

    let lighting_shader = shaders.push(Shader{
        shader: ShadersGroup::new(
            light_vertex::load,
            light_fragment::load
        ),
        depth: Some(DepthState{
            write_enable: true,
            compare_op: CompareOp::LessOrEqual
        }),
        per_vertex: Some(ObjectVertex::per_vertex()),
        subpass: 2,
        blend: Some(AttachmentBlend{
            src_color_blend_factor: BlendFactor::Zero,
            dst_color_blend_factor: BlendFactor::DstColor,
            color_blend_op: BlendOp::Add,
            src_alpha_blend_factor: BlendFactor::SrcAlpha,
            dst_alpha_blend_factor: BlendFactor::DstAlpha,
            alpha_blend_op: BlendOp::Min
        }),
        ..Default::default()
    });

    let temp = ();
    let ui_shader = /*shaders.push(Shader{
        shader: ShadersGroup::new(
            ui_vertex::load,
            ui_fragment::load
        ),
        per_vertex: Some(Object::per_vertex()),
        subpass: 1,
        ..Default::default()
    })*/lighting_shader;

    let final_mix_shader = shaders.push(Shader{
        shader: ShadersGroup::new(
            final_vertex::load,
            final_fragment::load
        ),
        per_vertex: Some(SimpleVertex::per_vertex()),
        subpass: 3,
        blend: None,
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
            occluder: occluder_shader,
            lighting: lighting_shader,
            ui: ui_shader,
            final_mix: final_mix_shader
        }
    }
}
