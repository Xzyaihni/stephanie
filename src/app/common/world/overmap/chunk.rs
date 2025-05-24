use std::{
    fmt::{self, Debug},
    ops::{Index, IndexMut}
};

use serde::{Serialize, Deserialize};
use serde_with::serde_as;

use nalgebra::Vector3;

use tile::Tile;

use crate::{impl_directionals, common::{Transform, world::debug_3d_slices}};
pub use pos::*;

pub mod tile;
pub mod pos;


pub const CHUNK_SIZE: usize = 16;
const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

pub const CHUNK_VISUAL_SIZE: f32 = CHUNK_SIZE as f32  * TILE_SIZE;

pub const TILE_SIZE: f32 = 0.1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChunkLocal(Pos3<usize>);

impl PartialEq for ChunkLocal
{
    fn eq(&self, other: &Self) -> bool
    {
        self.0 == other.0
    }
}

impl From<ChunkLocal> for Pos3<f32>
{
    fn from(value: ChunkLocal) -> Self
    {
        value.0.map(|x| x as f32 * TILE_SIZE)
    }
}

impl From<Pos3<usize>> for ChunkLocal
{
    fn from(value: Pos3<usize>) -> Self
    {
        let this = Self(value);

        debug_assert!(this.in_bounds());

        this
    }
}

impl_directionals!{ChunkLocal}

impl ChunkLocal
{
    pub fn new(x: usize, y: usize, z: usize) -> Self
    {
        Self::from(Pos3::new(x, y, z))
    }

    fn moved(&self, x: usize, y: usize, z: usize) -> Self
    {
        Self::new(x, y, z)
    }

    fn size(&self) -> Pos3<usize>
    {
        Pos3::repeat(CHUNK_SIZE)
    }

    pub fn pos_mut(&mut self) -> &mut Pos3<usize>
    {
        &mut self.0
    }

    pub fn pos(&self) -> &Pos3<usize>
    {
        &self.0
    }
}

#[serde_as]
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk
{
    #[serde_as(as = "Box<[_; CHUNK_VOLUME]>")]
    tiles: Box<[Tile; CHUNK_VOLUME]>
}

impl Debug for Chunk
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        debug_3d_slices(f, Pos3::repeat(CHUNK_SIZE), |pos|
        {
            self[ChunkLocal::from(pos)].id_string()
        })
    }
}

impl Chunk
{
    pub fn new() -> Self
    {
        let tiles = Box::new([Tile::none(); CHUNK_VOLUME]);

        Self{tiles}
    }

    pub fn new_with(f: impl FnMut(usize) -> Tile) -> Self
    {
        let tiles = Box::new((0..CHUNK_VOLUME).map(f).collect::<Vec<_>>().try_into().unwrap());

        Self{tiles}
    }

    #[must_use]
    pub fn with_set_tile(&self, pos: ChunkLocal, tile: Tile) -> Self
    {
        let mut new_chunk = self.clone();

        new_chunk[pos] = tile;

        new_chunk
    }

    pub fn position_of_chunk(pos: GlobalPos) -> Vector3<f32>
    {
        let chunk_pos = Pos3::<f32>::from(pos.0) * CHUNK_VISUAL_SIZE;

        Vector3::from(chunk_pos)
    }

    pub fn transform_of_chunk(pos: GlobalPos) -> Transform
    {
        Transform{
            position: Self::position_of_chunk(pos),
            ..Default::default()
        }
    }

    pub fn iter(&self) -> impl Iterator<Item=(ChunkLocal, &Tile)>
    {
        self.tiles.iter().enumerate().map(|(index, tile)| (Self::index_to_pos(index), tile))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item=(ChunkLocal, &mut Tile)>
    {
        self.tiles.iter_mut().enumerate().map(|(index, tile)| (Self::index_to_pos(index), tile))
    }

    pub fn index_to_pos(index: usize) -> ChunkLocal
    {
        ChunkLocal(Pos3::from_rectangle(Pos3::repeat(CHUNK_SIZE), index))
    }

    fn index_of(pos: ChunkLocal) -> usize
    {
        let pos = pos.0;

        pos.z * CHUNK_SIZE * CHUNK_SIZE + pos.y * CHUNK_SIZE + pos.x
    }
}

impl From<Box<[Tile; CHUNK_VOLUME]>> for Chunk
{
    fn from(value: Box<[Tile; CHUNK_VOLUME]>) -> Self
    {
        Self{tiles: value}
    }
}

impl Index<ChunkLocal> for Chunk
{
    type Output = Tile;

    fn index(&self, index: ChunkLocal) -> &Self::Output
    {
        &self.tiles[Self::index_of(index)]
    }
}

impl IndexMut<ChunkLocal> for Chunk
{
    fn index_mut(&mut self, index: ChunkLocal) -> &mut Self::Output
    {
        &mut self.tiles[Self::index_of(index)]
    }
}
