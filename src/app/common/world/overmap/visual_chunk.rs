use std::{
    iter,
    convert,
    ops::RangeInclusive,
    sync::Arc
};

use nalgebra::{Vector2, Vector3};

use yanyaengine::{
    Object,
    game_object::*
};

use crate::{
    client::tiles_factory::{
        ChunkSlice,
        TilesFactory,
        OccluderInfo,
        ChunkInfo,
        ChunkModelBuilder
    },
    common::{
        OccludingPlane,
        TileMap,
        world::{
            ChunkLocal,
            GlobalPos,
            MaybeGroup,
            Chunk,
            Tile,
            TILE_SIZE,
            CHUNK_SIZE,
            PosDirection,
            visual_overmap::TileReader
        }
    }
};


pub struct VisualChunkInfo
{
    infos: ChunkSlice<Box<[ChunkInfo]>>,
    occluders: ChunkSlice<Box<[OccluderInfo]>>,
    draw_height: ChunkSlice<usize>,
    draw_next: ChunkSlice<bool>
}

pub struct VisualChunk
{
    objects: ChunkSlice<Box<[Object]>>,
    occluders: ChunkSlice<Box<[OccludingPlane]>>,
    draw_height: ChunkSlice<usize>,
    draw_next: ChunkSlice<bool>,
    generated: bool
}

impl VisualChunk
{
    pub fn new() -> Self
    {
        let objects: ChunkSlice<Box<[Object]>> = Self::create_empty();
        let occluders: ChunkSlice<Box<[OccludingPlane]>> = Self::create_empty();

        Self{
            objects,
            occluders,
            draw_height: [0; CHUNK_SIZE],
            draw_next: [true; CHUNK_SIZE],
            generated: false
        }
    }

    fn create_empty<T>() -> ChunkSlice<Box<[T]>>
    {
        iter::repeat_with(||
        {
            let b: Box<[T]> = Box::new([]);

            b
        }).take(CHUNK_SIZE)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or_else(|_| unreachable!())
    }

    pub fn create(
        tilemap: Arc<TileMap>,
        mut model_builder: ChunkModelBuilder,
        pos: GlobalPos,
        tiles: TileReader
    ) -> VisualChunkInfo
    {
        let occluders = Self::create_occluders(
            pos,
            &tiles
        );

        let mut occlusions = [[false; CHUNK_SIZE * CHUNK_SIZE]; CHUNK_SIZE];

        for z in 0..CHUNK_SIZE
        {
            let slice_occlusions = &mut occlusions[z];

            for y in 0..CHUNK_SIZE
            {
                for x in 0..CHUNK_SIZE
                {
                    let pos = ChunkLocal::new(x, y, z);
                    let tiles = tiles.tile(pos);

                    let occluded = Self::create_tile(
                        &tilemap,
                        &mut model_builder,
                        pos,
                        tiles
                    );

                    slice_occlusions[y * CHUNK_SIZE + x] = occluded;
                }
            }
        }

        let infos = model_builder.build(pos);

        let (draw_next, draw_height) = Self::from_occlusions(&occlusions);

        VisualChunkInfo{
            infos,
            occluders,
            draw_height,
            draw_next
        }
    }

    pub fn build(
        tiles_factory: &mut TilesFactory,
        chunk_info: VisualChunkInfo
    ) -> Self
    {
        let objects = tiles_factory.build(chunk_info.infos);
        let occluders = tiles_factory.build_occluders(chunk_info.occluders);

        Self{
            objects,
            occluders,
            generated: true,
            draw_height: chunk_info.draw_height,
            draw_next: chunk_info.draw_next
        }
    }

    pub fn draw_next(&self, height: usize) -> bool
    {
        self.draw_next[height]
    }

    fn create_occluders(
        pos: GlobalPos,
        tiles: &TileReader
    ) -> ChunkSlice<Box<[OccluderInfo]>>
    {
        struct OccluderInfoRaw
        {
            position: Vector2<usize>,
            horizontal: bool,
            length: usize
        }

        let chunk_position = Chunk::transform_of_chunk(pos).position;

        (0..CHUNK_SIZE).map(|z|
        {
            let mut occluders = Vec::new();

            for y in 0..(CHUNK_SIZE + 1)
            {
                for x in 0..CHUNK_SIZE
                {
                    let occluder = OccluderInfoRaw{
                        position: Vector2::new(x, y),
                        horizontal: false,
                        length: 1
                    };

                    occluders.push(occluder);
                }
            }

            for x in 0..(CHUNK_SIZE + 1)
            {
                for y in 0..CHUNK_SIZE
                {
                    let occluder = OccluderInfoRaw{
                        position: Vector2::new(x, y),
                        horizontal: true,
                        length: 1
                    };

                    occluders.push(occluder);
                }
            }

            occluders.into_iter().map(|info: OccluderInfoRaw|
            {
                let tile_position = Vector3::new(info.position.x, info.position.y, z)
                    .cast() * TILE_SIZE;

                OccluderInfo{
                    position: chunk_position + tile_position,
                    horizontal: info.horizontal,
                    length: info.length as f32 * TILE_SIZE
                }
            }).collect::<Box<[_]>>()
        }).collect::<Vec<_>>().try_into().unwrap_or_else(|_| unreachable!())
    }

    fn from_occlusions(
        occlusions: &ChunkSlice<[bool; CHUNK_SIZE * CHUNK_SIZE]>
    ) -> (ChunkSlice<bool>, ChunkSlice<usize>)
    {
        let (next, height): (Vec<_>, Vec<_>) = (0..CHUNK_SIZE).map(|index|
        {
            let amount = Self::unoccluded_amount(occlusions[0..=index].iter().rev());

            let draw_next = amount > (index + 1);

            (draw_next, amount.min(index + 1))
        }).unzip();

        (next.try_into().unwrap(), height.try_into().unwrap())
    }

    fn unoccluded_amount<'a>(
        mut occlusions: impl Iterator<Item=&'a [bool; CHUNK_SIZE * CHUNK_SIZE]>
    ) -> usize
    {
        let mut current = if let Some(x) = occlusions.next()
        {
            x.to_vec()
        } else
        {
            return 0;
        };

        Self::unoccluded_amount_inner(&mut current, &mut occlusions)
    }

    fn unoccluded_amount_inner<'a>(
        current: &mut Vec<bool>,
        occlusions: &mut impl Iterator<Item=&'a [bool; CHUNK_SIZE * CHUNK_SIZE]>
    ) -> usize
    {
        let fully_occluded = current.iter().copied().all(convert::identity);

        if fully_occluded
        {
            1
        } else
        {
            if let Some(occlusion) = occlusions.next()
            {
                *current = current.iter().zip(occlusion.iter()).map(|(a, b)|
                {
                    *a || *b
                }).collect();

                1 + Self::unoccluded_amount_inner(current, occlusions)
            } else
            {
                2
            }
        }
    }

    fn create_tile(
        tilemap: &TileMap,
        model_builder: &mut ChunkModelBuilder,
        pos: ChunkLocal,
        tiles: MaybeGroup<Tile>
    ) -> bool
    {
        if tiles.this.is_none()
        {
            return false;
        }

        model_builder.create(pos, tiles.this);

        PosDirection::iter_non_z().for_each(|direction|
        {
            if let Some(gradient_tile) = tiles[direction]
            {
                if !tilemap[gradient_tile].transparent && gradient_tile != tiles.this
                {
                    model_builder.create_direction(
                        direction,
                        pos,
                        gradient_tile
                    );
                }
            }
        });

        let occluding = !tilemap[tiles.this].transparent;

        occluding
    }

    pub fn is_generated(&self) -> bool
    {
        self.generated
    }

    pub fn mark_ungenerated(&mut self)
    {
        self.generated = false;
    }

    fn draw_range(&self, height: usize) -> RangeInclusive<usize>
    {
        let draw_amount = self.draw_height[height];

        (height + 1 - draw_amount)..=height
    }

    pub fn update_buffers(&mut self, info: &mut UpdateBuffersInfo, height: usize)
    {
        let draw_range = self.draw_range(height);

        self.objects[draw_range].iter_mut().for_each(|objects|
        {
            objects.iter_mut().for_each(|object| object.update_buffers(info));
        });
    }

    pub fn draw(&self, info: &mut DrawInfo, height: usize)
    {
        let draw_range = self.draw_range(height);

        self.objects[draw_range].iter().for_each(|objects|
        {
            objects.iter().for_each(|object| object.draw(info));
        });
    }
}
