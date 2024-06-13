use std::{
    iter,
    convert,
    ops::RangeInclusive,
    sync::Arc
};

use yanyaengine::{
    Object,
    game_object::*
};

use crate::{
    client::tiles_factory::{TilesFactory, ChunkInfo, ChunkModelBuilder},
    common::{
        TileMap,
        world::{
            ChunkLocal,
            GlobalPos,
            MaybeGroup,
            Tile,
            CHUNK_SIZE,
            PosDirection,
            visual_overmap::TileReader
        }
    }
};


pub struct VisualChunkInfo
{
    infos: [Box<[ChunkInfo]>; CHUNK_SIZE],
    occlusions: [[bool; CHUNK_SIZE * CHUNK_SIZE]; CHUNK_SIZE]
}

#[derive(Debug)]
pub struct VisualChunk
{
    objects: [Box<[Object]>; CHUNK_SIZE],
    draw_height: [usize; CHUNK_SIZE],
    generated: bool
}

impl VisualChunk
{
    pub fn new() -> Self
    {
        let objects: [Box<[Object]>; CHUNK_SIZE] = iter::repeat_with(||
            {
                let b: Box<[Object]> = Box::new([]);

                b
            })
            .take(CHUNK_SIZE)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        Self{
            objects,
            draw_height: [0; CHUNK_SIZE],
            generated: false
        }
    }

    pub fn create(
        tilemap: Arc<TileMap>,
        mut model_builder: ChunkModelBuilder,
        pos: GlobalPos,
        tiles: TileReader
    ) -> VisualChunkInfo
    {
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

        VisualChunkInfo{
            occlusions,
            infos
        }
    }

    pub fn build(
        tiles_factory: &mut TilesFactory,
        chunk_info: VisualChunkInfo
    ) -> Self
    {
        let objects = tiles_factory.build(chunk_info.infos);

        let occlusions = chunk_info.occlusions;

        let draw_height = Self::from_occlusions(&occlusions);

        Self{
            objects,
            generated: true,
            draw_height
        }
    }

    fn from_occlusions(
        occlusions: &[[bool; CHUNK_SIZE * CHUNK_SIZE]; CHUNK_SIZE]
    ) -> [usize; CHUNK_SIZE]
    {
        (0..CHUNK_SIZE).map(|index|
        {
            let amount = Self::unoccluded_amount(occlusions[0..=index].iter().rev());

            amount.min(index + 1)
        }).collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }

    fn unoccluded_amount<'a>(
        mut occlusions: impl Iterator<Item=&'a [bool; CHUNK_SIZE * CHUNK_SIZE]>
    ) -> usize
    {
        let mut current_occlusion = if let Some(x) = occlusions.next()
        {
            x.to_vec()
        } else
        {
            return 0;
        };

        occlusions.take_while(|occlusion|
        {
            let fully_occluded = current_occlusion.iter().copied().all(convert::identity);

            current_occlusion = current_occlusion.iter().zip(occlusion.iter()).map(|(a, b)|
            {
                *a || *b
            }).collect();

            !fully_occluded
        }).count() + 1
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
