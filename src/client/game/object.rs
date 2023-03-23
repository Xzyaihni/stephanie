use std::{
    sync::Arc
};

use parking_lot::RwLock;

use vulkano::{
    memory::allocator::FastMemoryAllocator,
    pipeline::{
        PipelineBindPoint,
        layout::PipelineLayout
    },
    buffer::{
        BufferUsage,
        TypedBufferAccess,
        cpu_access::CpuAccessibleBuffer
    }
};

use nalgebra::base::Vector4;

use bytemuck::{Pod, Zeroable};

use super::{
    camera::Camera,
    object_transform::ObjectTransform
};

use crate::{
    client::BuilderType,
    common::{Transform, OnTransformCallback, TransformContainer}
};

use model::Model;
use texture::Texture;

pub mod resource_uploader;
pub mod model;
pub mod texture;


#[repr(C)]
#[derive(Debug, Default, Copy, Clone, Zeroable, Pod)]
pub struct Vertex
{
    position: [f32; 3],
    uv: [f32; 2]
}

vulkano::impl_vertex!(Vertex, position, uv);

#[derive(Debug)]
struct BufferContainer
{
    vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>
}

impl BufferContainer
{
    pub fn new(
        allocator: &FastMemoryAllocator,
        vertices: impl ExactSizeIterator<Item=Vertex>
    ) -> Self
    {
        let vertex_buffer = CpuAccessibleBuffer::from_iter(
            allocator,
            BufferUsage{
                vertex_buffer: true,
                ..Default::default()
            },
            false,
            vertices
        ).unwrap();

        Self{vertex_buffer}
    }
}

#[derive(Debug)]
pub struct Object
{
    camera: Arc<RwLock<Camera>>,
    model: Arc<Model>,
    texture: Arc<RwLock<Texture>>,
    transform: ObjectTransform,
    layout: Arc<PipelineLayout>,
    buffer_container: BufferContainer
}

#[allow(dead_code)]
impl Object
{
    pub fn new_default(
        allocator: FastMemoryAllocator,
        layout: Arc<PipelineLayout>,
        camera: Arc<RwLock<Camera>>,
        model: Arc<Model>,
        texture: Arc<RwLock<Texture>>
    ) -> Self
    {
        let transform = ObjectTransform::new_default();

        Self::new(allocator, layout, camera, model, texture, transform)
    }

    pub fn new(
        allocator: FastMemoryAllocator,
        layout: Arc<PipelineLayout>,
        camera: Arc<RwLock<Camera>>,
        model: Arc<Model>,
        texture: Arc<RwLock<Texture>>,
        transform: ObjectTransform
    ) -> Self
    {
        let vertices = Self::generate_vertices(&camera, &transform, &model);

        let buffer_container = BufferContainer::new(&allocator, vertices);

        Self{
            camera,
            model,
            texture,
            transform,
            layout,
            buffer_container
        }
    }

    pub fn regenerate_buffers(&mut self, allocator: &FastMemoryAllocator)
    {
        let vertices =
            Self::generate_vertices(&self.camera, &self.transform, &self.model);

        self.buffer_container = BufferContainer::new(
            allocator,
            vertices
        );
    }

    fn generate_vertices<'a>(
        camera: &Arc<RwLock<Camera>>,
        transform: &ObjectTransform,
        model: &'a Arc<Model>
    ) -> impl ExactSizeIterator<Item=Vertex> + 'a
    {
        let projection_view = camera.read().projection_view();
        let transform = transform.matrix();

        model.vertices.iter().zip(model.uvs.iter()).map(move |(vertex, uv)|
        {
            let vertex = Vector4::new(vertex[0], vertex[1], vertex[2], 1.0);

            let vertex = projection_view * transform * vertex;

            Vertex{position: vertex.xyz().into(), uv: *uv}
        })
    }

    pub fn draw(&self, builder: BuilderType)
    {
        let vertex_buffer = &self.buffer_container.vertex_buffer;

        builder
            .bind_descriptor_sets(
                PipelineBindPoint::Graphics,
                self.layout.clone(),
                0,
                self.texture.read().descriptor_set()
            )
            .bind_vertex_buffers(0, vertex_buffer.clone())
            .draw(vertex_buffer.len() as u32, 1, 0, 0)
            .unwrap();
    }
}

impl OnTransformCallback for Object
{
    fn callback(&mut self)
    {
        self.transform.callback();
    }
}

impl TransformContainer for Object
{
    fn transform_ref(&self) -> &Transform
    {
        self.transform.transform_ref()
    }

    fn transform_mut(&mut self) -> &mut Transform
    {
        self.transform.transform_mut()
    }
}