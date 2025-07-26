use std::{
    cmp::Ordering,
    sync::Arc,
    collections::{HashMap, BinaryHeap}
};

use nalgebra::{Vector2, Vector3};

use vulkano::{
    buffer::subbuffer::BufferContents,
    pipeline::graphics::vertex_input::Vertex
};

use yanyaengine::{Transform, game_object::*};

use crate::{
    client::{
        VisibilityChecker,
        TilesFactory,
        world_receiver::WorldReceiver
    },
    common::{
        some_or_return,
        collider::*,
        raycast,
        TileMap,
        TileInfo,
        Entity,
        OccludingCaster,
        entity::ClientEntities,
        message::Message
    }
};

pub use overmap::{
    Overmap,
    FlatChunksContainer,
    ChunksContainer,
    chunks_container::debug_3d_slices,
    chunk::{
        self,
        CHUNK_SIZE,
        CHUNK_VISUAL_SIZE,
        TILE_SIZE,
        Axis,
        Pos3,
        Chunk,
        ChunkLocal,
        GlobalPos,
        LocalPos,
        PosDirection,
        DirectionsGroup,
        Directions3dGroup,
        MaybeGroup,
        AlwaysGroup,
        tile::{Tile, TileExisting, TileRotation}
    }
};

pub use client_overmap::TilePos;

use client_overmap::ClientOvermap;
use visual_overmap::VisualOvermap;

use pathfind::WorldPath;

pub mod overmap;

mod client_overmap;
mod visual_overmap;

pub mod pathfind;


pub const CLIENT_OVERMAP_SIZE: usize = 8;
pub const CLIENT_OVERMAP_SIZE_Z: usize = 3;

const PATHFIND_MAX_STEPS: usize = 1000;

#[derive(BufferContents, Vertex, Debug, Clone, Copy)]
#[repr(C)]
pub struct SkyOccludingVertex
{
    #[format(R32G32_SFLOAT)]
    pub position: [f32; 2]
}

impl From<([f32; 4], [f32; 2])> for SkyOccludingVertex
{
    fn from(([x, y, _z, _w], _uv): ([f32; 4], [f32; 2])) -> Self
    {
        Self{position: [x, y]}
    }
}

#[derive(BufferContents, Vertex, Debug, Clone, Copy)]
#[repr(C)]
pub struct SkyLightVertex
{
    #[format(R32G32_SFLOAT)]
    pub position: [f32; 2],
    #[format(R32_SFLOAT)]
    pub intensity: f32
}

impl From<([f32; 4], [f32; 2])> for SkyLightVertex
{
    fn from(([x, y, _z, _w], [u, _v]): ([f32; 4], [f32; 2])) -> Self
    {
        Self{position: [x, y], intensity: u}
    }
}

#[derive(Debug, Clone)]
pub struct ChunkWithEntities
{
    pub chunk: Chunk,
    pub entities: Vec<Entity>
}

pub struct World
{
    tilemap: Arc<TileMap>,
    world_receiver: WorldReceiver,
    overmap: ClientOvermap
}

impl World
{
    pub fn new(
        world_receiver: WorldReceiver,
        tiles_factory: TilesFactory,
        camera_size: Vector2<f32>,
        player_position: Pos3<f32>
    ) -> Self
    {
        let tilemap = tiles_factory.tilemap().clone();
        let size = Self::overmap_size();

        let visual_overmap = VisualOvermap::new(tiles_factory, size, camera_size, player_position);
        let overmap = ClientOvermap::new(
            world_receiver.clone(),
            visual_overmap,
            size,
            player_position
        );

        Self{tilemap, world_receiver, overmap}
    }

    pub fn tilemap(&self) -> &TileMap
    {
        &self.tilemap
    }

    pub fn tile_info(&self, tile: Tile) -> &TileInfo
    {
        self.tilemap.info(tile)
    }

    pub fn overmap_size() -> Pos3<usize>
    {
        Pos3::new(CLIENT_OVERMAP_SIZE, CLIENT_OVERMAP_SIZE, CLIENT_OVERMAP_SIZE_Z)
    }

    pub fn zoom_limit() -> f32
    {
        //make the camera smaller by 3 tiles so theres time for the missing chunks to load
        let padding = 3;

        let padding = TILE_SIZE * padding as f32;

        let max_scale = (CLIENT_OVERMAP_SIZE - 1) as f32 * CHUNK_VISUAL_SIZE - padding;

        max_scale
    }

    pub fn exists_missing(&self) -> (u32, u32)
    {
        self.overmap.exists_missing()
    }

    fn chunk_of(pos: Pos3<f32>) -> GlobalPos
    {
        TilePos::from(pos).chunk
    }

    pub fn pathfind(
        &self,
        scale: Vector3<f32>,
        start: Vector3<f32>,
        end: Vector3<f32>
    ) -> Option<WorldPath>
    {
        let target = TilePos::from(end);
        let start = TilePos::from(start);

        if start.distance(target).z > 0
        {
            return None;
        }

        struct NodeInfo
        {
            moves_from_start: u32,
            previous: Option<Node>
        }

        #[derive(Debug, Clone)]
        struct Node
        {
            cost: f32,
            value: TilePos
        }

        impl Node
        {
            fn path_to<T, F: Fn(TilePos) -> T>(
                self,
                explored: &mut HashMap<TilePos, NodeInfo>,
                path: &mut Vec<T>,
                f: F
            )
            {
                if let Some(node) = explored.remove(&self.value).unwrap().previous
                {
                    path.push(f(node.value));
                    node.path_to(explored, path, f);
                }
            }
        }

        impl PartialEq for Node
        {
            fn eq(&self, other: &Self) -> bool
            {
                self.cost.eq(&other.cost)
            }
        }

        impl Eq for Node {}

        impl PartialOrd for Node
        {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering>
            {
                other.cost.partial_cmp(&self.cost)
            }
        }

        impl Ord for Node
        {
            fn cmp(&self, other: &Self) -> Ordering { self.partial_cmp(other).unwrap() }
        }

        let mut steps = 0;

        let mut unexplored = BinaryHeap::from([
            Node{cost: 0.0, value: start}
        ]);

        let mut explored = HashMap::from([(start, NodeInfo{moves_from_start: 0, previous: None})]);

        while !unexplored.is_empty()
        {
            steps += 1;
            if steps > PATHFIND_MAX_STEPS
            {
                return None;
            }

            let current = unexplored.pop()?;

            if current.value == target
            {
                let tiles = {
                    let current_position: Vector3<f32> = current.value.center_position().into();
                    let mut path = vec![Vector3::new(end.x, end.y, current_position.z), current_position];
                    current.path_to(&mut explored, &mut path, |x| x.center_position().into());

                    path
                };

                let mut check = 0;

                let mut simplified = vec![tiles[0]];

                let mut index = 1;
                while index < tiles.len()
                {
                    let is_next = (check + 1) == index;

                    let is_tile_reachable = |tiles: &[Vector3<f32>]|
                    {
                        let distance = tiles[index] - tiles[check];

                        let start = Vector3::from(tiles[check]);

                        raycast::swept_aabb_world(
                            self,
                            &Transform{
                                position: start,
                                scale,
                                ..Default::default()
                            },
                            distance
                        ).is_none()
                    };

                    let is_reachable = is_next || is_tile_reachable(&tiles);

                    if is_reachable
                    {
                        index += 1;
                    } else
                    {
                        check = index - 1;

                        simplified.push(tiles[check]);
                    }
                }

                return Some(WorldPath::new(simplified));
            }

            let below = current.value.offset(Pos3::new(0, 0, -1));
            let is_grounded = !self.tile(below)?.is_none();

            let mut try_push = |position: TilePos|
            {
                let moves_from_start = explored[&current.value].moves_from_start;

                if let Some(explored) = explored.get_mut(&position)
                {
                    if explored.moves_from_start > moves_from_start + 1
                    {
                        explored.moves_from_start = moves_from_start + 1;
                        explored.previous = Some(current.clone());
                    }
                } else
                {
                    let moves_from_start = moves_from_start + 1;

                    let info = NodeInfo{moves_from_start, previous: Some(current.clone())};
                    explored.insert(position, info);

                    let goal_distance = Vector3::from(position.distance(target)).cast::<f32>().magnitude();

                    let cost = moves_from_start as f32 + goal_distance;

                    unexplored.push(Node{
                        cost,
                        value: position
                    });
                }
            };

            if is_grounded
            {
                PosDirection::iter_non_z().for_each(|direction|
                {
                    let position = current.value.offset(Pos3::from(direction));

                    if self.tile(position).map(|x| x.is_none()).unwrap_or(false)
                    {
                        try_push(position);
                    }
                });
            } else
            {
                try_push(below);
            }
        }

        None
    }

    pub fn inside_chunk(&self, pos: Pos3<f32>) -> bool
    {
        self.overmap.contains(Self::chunk_of(pos))
    }

    pub fn debug_visual_overmap(&self)
    {
        self.overmap.debug_visual_overmap();
    }

    pub fn debug_chunk(&self, pos: Pos3<f32>, visual: bool) -> String
    {
        self.overmap.debug_chunk(Self::chunk_of(pos), visual)
    }

    pub fn debug_tile_occlusion(&self, entities: &ClientEntities)
    {
        self.overmap.debug_tile_occlusion(entities)
    }

    pub fn tiles_inside<'a, Predicate>(
        &self,
        collider: &'a CollidingInfo<'a>,
        predicate: Predicate
    ) -> impl Iterator<Item=TilePos> + use<'a, '_, Predicate>
    where
        Predicate: Fn(Option<&Tile>) -> bool + Copy
    {
        self.tiles_inside_inner(
            collider,
            predicate,
            |info| collider.collide_immutable(&info, |_| {}),
            false
        )
    }

    pub fn tiles_contacts<'a, ContactAdder, Predicate>(
        &self,
        collider: &'a CollidingInfo<'a>,
        mut add_contact: ContactAdder,
        predicate: Predicate
    ) -> impl Iterator<Item=TilePos> + use<'a, '_, Predicate, ContactAdder>
    where
        ContactAdder: FnMut(Contact),
        Predicate: Fn(Option<&Tile>) -> bool + Copy
    {
        self.tiles_inside_inner(
            collider,
            predicate,
            move |info| collider.collide_immutable(&info, &mut add_contact),
            true
        )
    }

    fn tiles_inside_inner<'a, CheckCollision, Predicate>(
        &self,
        collider: &'a CollidingInfo<'a>,
        predicate: Predicate,
        mut check_collision: CheckCollision,
        check_neighbors: bool
    ) -> impl Iterator<Item=TilePos> + use<'a, '_, Predicate, CheckCollision>
    where
        CheckCollision: FnMut(CollidingInfo) -> bool,
        Predicate: Fn(Option<&Tile>) -> bool + Copy
    {
        let half_scale = collider.bounds();

        let top_left = TilePos::from(collider.transform.position - half_scale);
        let bottom_right = TilePos::from(collider.transform.position + half_scale);

        top_left.tiles_between(bottom_right).filter(move |pos|
        {
            predicate(self.tile(*pos))
        }).filter(move |pos|
        {
            let check_tile = |pos|
            {
                predicate(self.tile(pos))
            };

            let world = if check_neighbors
            {
                Directions3dGroup{
                    left: check_tile(pos.offset(Pos3::new(-1, 0, 0))),
                    right: check_tile(pos.offset(Pos3::new(1, 0, 0))),
                    down: check_tile(pos.offset(Pos3::new(0, -1, 0))),
                    up: check_tile(pos.offset(Pos3::new(0, 1, 0))),
                    back: check_tile(pos.offset(Pos3::new(0, 0, -1))),
                    forward: check_tile(pos.offset(Pos3::new(0, 0, 1)))
                }
            } else
            {
                Directions3dGroup::repeat(false)
            };

            let mut world_collider = ColliderInfo{
                kind: ColliderType::Tile(world),
                layer: ColliderLayer::World,
                ghost: false,
                scale: None
            }.into();

            let info = CollidingInfo{
                entity: None,
                transform: Transform{
                    position: pos.entity_position(),
                    scale: Vector3::repeat(TILE_SIZE),
                    ..Default::default()
                },
                collider: &mut world_collider
            };

            check_collision(info)
        })
    }

    pub fn modify_tile(&mut self, pos: TilePos, f: impl FnOnce(&mut Self, &mut Tile))
    {
        let tile: Tile = *some_or_return!(self.tile(pos));
        let mut new_tile: Tile = tile;

        f(self, &mut new_tile);

        if tile != new_tile
        {
            self.set_tile(pos, new_tile);
        }
    }

    pub fn tile(&self, index: TilePos) -> Option<&Tile>
    {
        self.overmap.tile(index)
    }

    pub fn set_tile(&mut self, pos: TilePos, tile: Tile)
    {
        if self.set_tile_local(pos, tile)
        {
            self.world_receiver.set_tile(pos, tile);
        }
    }

    fn set_tile_local(&mut self, pos: TilePos, new_tile: Tile) -> bool
    {
        if self.tile(pos).copied() == Some(new_tile)
        {
            return false;
        }

        self.overmap.set_tile(pos, new_tile);

        true
    }

    pub fn update(&mut self, dt: f32)
    {
        self.overmap.update(dt);
    }

    pub fn rescale(&mut self, size: Vector2<f32>)
    {
        self.overmap.rescale(size);
    }

    pub fn camera_position(&self) -> Pos3<f32>
    {
        self.overmap.camera_position()
    }

    pub fn camera_moved(&mut self, pos: Pos3<f32>)
    {
        self.overmap.camera_moved(pos);
    }

    pub fn handle_message(&mut self, message: Message) -> Option<Message>
    {
        match message
        {
            Message::SetTile{pos, tile} =>
            {
                self.set_tile_local(pos, tile);
                None
            },
            Message::ChunkSync{pos, chunk} =>
            {
                self.overmap.set(pos, chunk);
                None
            },
            _ => Some(message)
        }
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        self.overmap.update_buffers(info);
    }

    pub fn update_buffers_shadows(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &VisibilityChecker,
        caster: &OccludingCaster
    )
    {
        self.overmap.update_buffers_shadows(info, visibility, caster);
    }

    pub fn sky_occluded(&self, transform: &Transform) -> bool
    {
        self.overmap.sky_occluded(transform)
    }

    pub fn wall_occluded(&self, transform: &Transform) -> bool
    {
        self.overmap.wall_occluded(transform)
    }

    pub fn update_buffers_light_shadows(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &VisibilityChecker,
        caster: &OccludingCaster,
        id: usize
    )
    {
        self.overmap.update_buffers_light_shadows(info, visibility, caster, id)
    }

    pub fn draw_shadows(
        &self,
        info: &mut DrawInfo
    )
    {
        self.overmap.draw_shadows(info);
    }

    pub fn draw_light_shadows(
        &self,
        info: &mut DrawInfo,
        visibility: &VisibilityChecker,
        id: usize,
        f: impl FnOnce(&mut DrawInfo)
    )
    {
        self.overmap.draw_light_shadows(info, visibility, id, f);
    }

    pub fn draw_sky_occluders(
        &self,
        info: &mut DrawInfo
    )
    {
        self.overmap.draw_sky_occluders(info);
    }

    pub fn draw_sky_lights(
        &self,
        info: &mut DrawInfo
    )
    {
        self.overmap.draw_sky_lights(info);
    }

    pub fn draw_tiles(
        &self,
        info: &mut DrawInfo,
        is_shaded: bool
    )
    {
        self.overmap.draw_tiles(info, is_shaded);
    }
}
