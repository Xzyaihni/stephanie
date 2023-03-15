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
    command_buffer::{AutoCommandBufferBuilder, PrimaryAutoCommandBuffer},
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

use crate::common::{Transform, TransformContainer};

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
    pub fn new(allocator: &FastMemoryAllocator, vertices: &[Vertex]) -> Self
    {
        let vertex_buffer = CpuAccessibleBuffer::from_iter(
            allocator,
            BufferUsage{
                vertex_buffer: true,
                ..Default::default()
            },
            false,
            vertices.iter().cloned()
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
    vertices: Vec<Vertex>,
    transform: ObjectTransform,
    layout: Arc<PipelineLayout>,
    allocator: FastMemoryAllocator,
    buffer_container: BufferContainer
}

impl Object
{
    pub fn new(
        allocator: FastMemoryAllocator,
        layout: Arc<PipelineLayout>,
        camera: Arc<RwLock<Camera>>,
        model: Arc<Model>,
        texture: Arc<RwLock<Texture>>
    ) -> Self
    {
        let transform = ObjectTransform::new();

        let vertices = Self::generate_vertices(&camera, &transform, &model);

        let buffer_container = BufferContainer::new(&allocator, &vertices);

        Self{camera, model, texture, vertices, transform, layout, allocator, buffer_container}
    }

    pub fn regenerate_buffer(&mut self)
    {
        self.vertices =
            Self::generate_vertices(&self.camera, &self.transform, &self.model);

        self.buffer_container = BufferContainer::new(
            &self.allocator,
            &self.vertices
        );
    }

    fn generate_vertices(
        camera: &Arc<RwLock<Camera>>,
        transform: &ObjectTransform,
        model: &Arc<Model>
    ) -> Vec<Vertex>
    {
        let projection_view = camera.read().projection_view();
        let transform = transform.matrix();

        model.vertices.iter().zip(model.uvs.iter()).map(|(vertex, uv)|
        {
            let vertex = Vector4::new(vertex[0], vertex[1], vertex[2], 1.0);

            let vertex = projection_view * transform * vertex;

            Vertex{position: vertex.xyz().into(), uv: *uv}
        }).collect()
    }

    pub fn draw(&self, builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>)
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

    fn callback(&mut self)
    {
        self.transform.callback();
    }
}