use std::f32;

use nalgebra::{Vector2, Vector3};

use serde::{Serialize, Deserialize};

use vulkano::{
    buffer::subbuffer::BufferContents,
    pipeline::graphics::vertex_input::Vertex
};

use yanyaengine::{
    Transform,
    TransformContainer,
    OccludingPlane as OccludingPlaneGeneric,
    OccluderPoints,
    game_object::*
};

use crate::{
    client::{VisibilityChecker, RenderCreateInfo},
    common::{rotate_point_z_3d, some_or_value, line_on_left, ServerToClient, world::TILE_SIZE}
};



#[derive(BufferContents, Vertex, Debug, Clone, Copy)]
#[repr(C)]
pub struct OccludingVertex
{
    #[format(R32G32B32_SFLOAT)]
    pub position: [f32; 3]
}

impl From<[f32; 4]> for OccludingVertex
{
    fn from([x, y, _z, w]: [f32; 4]) -> Self
    {
        Self{position: [x, y, w]}
    }
}

impl From<([f32; 4], [f32; 2])> for OccludingVertex
{
    fn from((position, _uv): ([f32; 4], [f32; 2])) -> Self
    {
        Self::from(position)
    }
}

pub type OccludingPlaneInner = OccludingPlaneGeneric<OccludingVertex>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Occluder
{
    Door
}

#[derive(Debug)]
pub enum ClientOccluder
{
    Door([OccludingPlane; 3])
}

impl ClientOccluder
{
    fn door_transforms(transform: Transform) -> [(Transform, bool); 3]
    {
        let scale = transform.scale;
        let rotation = transform.rotation;
        let world_offset = |x: Vector3<f32>|
        {
            rotate_point_z_3d(x.component_mul(&scale), rotation)
        };

        let top = Transform{
            position: transform.position + world_offset(-Vector3::y() * 0.5),
            ..transform
        };

        let bottom = Transform{
            position: transform.position + world_offset(Vector3::y() * 0.5),
            ..transform
        };

        let right = Transform{
            position: transform.position + world_offset(Vector3::x() * 0.5),
            rotation: transform.rotation + f32::consts::FRAC_PI_2,
            scale: scale.yxz(),
            ..transform
        };

        [(top, false), (bottom, true), (right, false)]
    }

    pub fn set_transform(&mut self, transform: Transform)
    {
        match self
        {
            Self::Door(planes) =>
            {
                planes.iter_mut().zip(Self::door_transforms(transform)).for_each(|(x, (target, _))|
                {
                    x.set_transform(target);
                });
            }
        }
    }

    pub fn visible(&self, visibility: &VisibilityChecker) -> bool
    {
        if !self.visible_height(visibility)
        {
            return false;
        }

        match self
        {
            Self::Door(planes) => planes.iter().any(|x| x.visible(visibility))
        }
    }

    pub fn visible_height(&self, visibility: &VisibilityChecker) -> bool
    {
        match self
        {
            Self::Door(planes) =>
            {
                planes[0].visible_height(visibility)
            }
        }
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo,
        caster: &OccludingCaster
    )
    {
        match self
        {
            Self::Door(planes) => planes.iter_mut().for_each(|x| x.update_buffers(info, caster))
        }
    }

    pub fn draw(
        &self,
        info: &mut DrawInfo
    )
    {
        match self
        {
            Self::Door(planes) => planes.iter().for_each(|x| x.draw(info))
        }
    }
}

impl ServerToClient<ClientOccluder> for Occluder
{
    fn server_to_client(
        self,
        transform: impl FnOnce() -> Transform,
        create_info: &mut RenderCreateInfo
    ) -> ClientOccluder
    {
        let create_plane = |(transform, reverse)|
        {
            let inner = create_info.object_info.partial.object_factory.create_occluding(transform, reverse);

            OccludingPlane(inner)
        };

        match self
        {
            Self::Door =>
            {
                let transforms = ClientOccluder::door_transforms(transform());

                ClientOccluder::Door(transforms.map(create_plane))
            }
        }
    }
}

pub struct OccludingCaster(Vector3<f32>);

impl From<Vector3<f32>> for OccludingCaster
{
    fn from(value: Vector3<f32>) -> Self
    {
        Self(value)
    }
}

#[derive(Debug)]
pub struct OccludingPlane(OccludingPlaneInner);

impl OccludingPlane
{
    pub fn new(inner: OccludingPlaneInner) -> Self
    {
        OccludingPlane(inner)
    }

    pub fn set_transform(&mut self, transform: Transform)
    {
        self.0.set_transform(transform);
    }

    pub fn points(&self) -> &Option<OccluderPoints>
    {
        self.0.points()
    }

    pub fn occludes_point(&self, point: Vector2<f32>) -> bool
    {
        let OccluderPoints{
            bottom_left,
            bottom_right,
            top_left,
            top_right
        } = some_or_value!(self.0.points(), false);

        let reverse = self.0.reverse_winding();

        let infront = line_on_left(
            point,
            if reverse { *bottom_right } else { *bottom_left },
            if reverse { *bottom_left } else { *bottom_right }
        );

        if !infront
        {
            return false;
        }

        let between_left = line_on_left(
            point,
            if reverse { *bottom_left } else { *top_left },
            if reverse { *top_left } else { *bottom_left }
        );

        if !between_left
        {
            return false;
        }

        line_on_left(
            point,
            if reverse { *top_right } else { *bottom_right },
            if reverse { *bottom_right } else { *top_right }
        )
    }

    pub fn visible(&self, visibility: &VisibilityChecker) -> bool
    {
        Self::visible_with(visibility, self.0.transform_ref())
    }

    pub fn visible_height(&self, visibility: &VisibilityChecker) -> bool
    {
        let top = visibility.position.z + visibility.size.z / 2.0;
        (0.0..TILE_SIZE).contains(&(top - self.0.transform_ref().position.z))
    }

    pub fn visible_with(visibility: &VisibilityChecker, transform: &Transform) -> bool
    {
        visibility.visible_occluding_plane(transform)
    }

    pub fn is_visible(&self) -> bool
    {
        !self.0.is_back()
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo,
        caster: &OccludingCaster
    )
    {
        self.0.update_buffers(caster.0, info);
    }

    pub fn draw(
        &self,
        info: &mut DrawInfo
    )
    {
        self.0.draw(info);
    }
}
