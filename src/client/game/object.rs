use std::{
    fmt,
    sync::Arc
};

use parking_lot::RwLock;

use vulkano::{
    pipeline::{
        PipelineBindPoint,
        graphics::vertex_input::Vertex
    },
    buffer::Subbuffer
};

use nalgebra::{Vector3, Vector4};

use bytemuck::{Pod, Zeroable};

use super::{
    camera::Camera,
    object_transform::ObjectTransform
};

use crate::{
    client::{GameObject, game_object_types::*, ObjectAllocator},
    common::{Transform, OnTransformCallback, TransformContainer}
};

use model::Model;
use texture::Texture;

pub mod resource_uploader;
pub mod model;
pub mod texture;


#[repr(C)]
#[derive(Vertex, Debug, Default, Copy, Clone, Zeroable, Pod)]
pub struct ObjectVertex
{
    #[format(R32G32B32_SFLOAT)]
    position: [f32; 3],

    #[format(R32G32_SFLOAT)]
    uv: [f32; 2]
}

pub trait DrawableEntity
{
    fn texture(&self) -> &str;
}

pub struct Object
{
    camera: Arc<RwLock<Camera>>,
    model: Arc<RwLock<Model>>,
    texture: Arc<RwLock<Texture>>,
    transform: ObjectTransform,
    subbuffers: Box<[Subbuffer<[ObjectVertex]>]>
}

#[allow(dead_code)]
impl Object
{
    pub fn new_default(
        camera: Arc<RwLock<Camera>>,
        model: Arc<RwLock<Model>>,
        texture: Arc<RwLock<Texture>>,
        allocator: &ObjectAllocator
    ) -> Self
    {
        let transform = ObjectTransform::new_default();

        Self::new(camera, model, texture, transform, allocator)
    }

    pub fn new(
        camera: Arc<RwLock<Camera>>,
        model: Arc<RwLock<Model>>,
        texture: Arc<RwLock<Texture>>,
        transform: ObjectTransform,
        allocator: &ObjectAllocator
    ) -> Self
    {
        let subbuffers = allocator.subbuffers(&model.read());

        let mut this = Self{
            camera,
            model,
            texture,
            transform,
            subbuffers
        };

        (0..allocator.subbuffers_amount()).for_each(|index| this.update_buffer(index));

        this
    }


    fn calculate_vertices(&self) -> Box<[ObjectVertex]>
    {
        let projection_view = self.camera.read().projection_view();
        let transform = self.transform.matrix();

        let model = self.model.read();

        model.vertices.iter().zip(model.uvs.iter()).map(move |(vertex, uv)|
        {
            let vertex = Vector4::new(vertex[0], vertex[1], vertex[2], 1.0);

            let vertex = projection_view * transform * vertex;

            ObjectVertex{position: vertex.xyz().into(), uv: *uv}
        }).collect::<Vec<_>>().into_boxed_slice()
    }

    fn update_buffer(&mut self, index: usize)
    {
        let vertices = self.calculate_vertices();

        self.subbuffers[index].write().unwrap().copy_from_slice(&vertices);
    }

    pub fn set_origin(&mut self, origin: Vector3<f32>)
    {
        self.transform.set_origin(origin);
    }
}

impl GameObject for Object
{
    fn update(&mut self, _dt: f32) {}

    fn update_buffers(&mut self, builder: BuilderType, index: usize)
    {
        builder.update_buffer(self.subbuffers[index].clone(), self.calculate_vertices()).unwrap();
    }

    fn draw(&self, builder: BuilderType, layout: LayoutType, index: usize)
    {
        let size = self.model.read().vertices.len() as u32;

        builder
            .bind_descriptor_sets(
                PipelineBindPoint::Graphics,
                layout,
                0,
                self.texture.read().descriptor_set()
            )
            .bind_vertex_buffers(0, self.subbuffers[index].clone())
            .draw(size, 1, 0, 0)
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