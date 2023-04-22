use std::{
    fmt,
    sync::Arc
};

use parking_lot::RwLock;

use vulkano::{
    memory::allocator::StandardMemoryAllocator,
    pipeline::PipelineBindPoint,
    buffer::{
        BufferUsage,
        TypedBufferAccess,
        cpu_access::CpuAccessibleBuffer
    }
};

use nalgebra::{Vector3, Vector4};

use bytemuck::{Pod, Zeroable};

use super::{
    camera::Camera,
    object_transform::ObjectTransform
};

use crate::{
    client::{GameObject, game_object_types::*},
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

pub trait DrawableEntity
{
    fn texture(&self) -> &str;
}

pub struct Object
{
    camera: Arc<RwLock<Camera>>,
    model: Arc<RwLock<Model>>,
    texture: Arc<RwLock<Texture>>,
    transform: ObjectTransform
}

#[allow(dead_code)]
impl Object
{
    pub fn new_default(
        camera: Arc<RwLock<Camera>>,
        model: Arc<RwLock<Model>>,
        texture: Arc<RwLock<Texture>>
    ) -> Self
    {
        let transform = ObjectTransform::new_default();

        Self::new(camera, model, texture, transform)
    }

    pub fn new(
        camera: Arc<RwLock<Camera>>,
        model: Arc<RwLock<Model>>,
        texture: Arc<RwLock<Texture>>,
        transform: ObjectTransform
    ) -> Self
    {
        Self{
            camera,
            model,
            texture,
            transform
        }
    }


    fn vertex_buffer(
        &self,
        allocator: &StandardMemoryAllocator
    ) -> Arc<CpuAccessibleBuffer<[Vertex]>>
    {
        let projection_view = self.camera.read().projection_view();
        let transform = self.transform.matrix();

        let model = self.model.read();

        let vertices = model.vertices.iter().zip(model.uvs.iter()).map(move |(vertex, uv)|
        {
            let vertex = Vector4::new(vertex[0], vertex[1], vertex[2], 1.0);

            let vertex = projection_view * transform * vertex;

            Vertex{position: vertex.xyz().into(), uv: *uv}
        });

        CpuAccessibleBuffer::from_iter(
            allocator,
            BufferUsage{
                vertex_buffer: true,
                ..Default::default()
            },
            false,
            vertices
        ).unwrap()
    }

    pub fn set_origin(&mut self, origin: Vector3<f32>)
    {
        self.transform.set_origin(origin);
    }
}

impl GameObject for Object
{
    fn update(&mut self, _dt: f32) {}

    fn draw(&self, allocator: AllocatorType, builder: BuilderType, layout: LayoutType)
    {
        let vertex_buffer = self.vertex_buffer(allocator);
        let vertices_len = vertex_buffer.len() as u32;

        builder
            .bind_descriptor_sets(
                PipelineBindPoint::Graphics,
                layout,
                0,
                self.texture.read().descriptor_set()
            )
            .bind_vertex_buffers(0, vertex_buffer)
            .draw(vertices_len, 1, 0, 0)
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

impl fmt::Debug for Object
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        f.debug_struct("Object")
            .field("camera", &self.camera)
            .field("model", &self.model)
            .field("texture", &self.texture)
            .field("transform", &self.transform)
            .finish()
    }
}