use std::{
    thread::{self, JoinHandle},
    iter,
    time::Instant,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender}
    }
};

use parking_lot::RwLock;

use nalgebra::{Vector2, Vector3};

use yanyaengine::{game_object::*, Transform};

use crate::{
    client::{VisibilityChecker as EntityVisibilityChecker, TilesFactory},
    common::{
        aabb_points,
        SortableF32,
        render_info::*,
        OccludingCaster,
        AnyEntities,
        EntityInfo,
        OccluderVisibilityChecker,
        watcher::Watchers,
        entity::ClientEntities
    }
};

use super::{
    TILE_SIZE,
    chunk::{
        CHUNK_SIZE,
        CHUNK_VISUAL_SIZE,
        Pos3,
        Chunk,
        ChunkLocal,
        MaybeGroup,
        GlobalPos,
        LocalPos,
        tile::Tile
    },
    overmap::{
        OvermapIndexing,
        CommonIndexing,
        ChunksContainer,
        visual_chunk::{VisualChunk, VisualChunkInfo, OccluderCached}
    }
};


struct VisualGenerated
{
    chunk_info: VisualChunkInfo,
    position: GlobalPos,
    timestamp: Instant
}

#[derive(Debug, Clone)]
struct VisibilityChecker
{
    pub size: Pos3<usize>,
    pub camera_size: Vector2<f32>,
    pub player_position: Arc<RwLock<Pos3<f32>>>
}

impl VisibilityChecker
{
    pub fn new(
        size: Pos3<usize>,
        camera_size: Vector2<f32>,
        player_position: Pos3<f32>
    ) -> Self
    {
        let player_position = Arc::new(RwLock::new(player_position));

        Self{size, camera_size, player_position}
    }

    pub fn visible(&self, pos: LocalPos) -> bool
    {
        let player_offset = self.player_offset();

        let offset_position = Pos3::from(pos) - Pos3::from(self.size / 2);

        let chunk_offset = offset_position * CHUNK_VISUAL_SIZE - player_offset;

        let in_range = |value: f32, limit: f32| -> bool
        {
            let limit = limit / 2.0;

            ((-limit - CHUNK_VISUAL_SIZE)..=limit).contains(&value)
        };

        in_range(chunk_offset.x, self.camera_size.x)
            && in_range(chunk_offset.y, self.camera_size.y)
    }

    fn player_offset(&self) -> Pos3<f32>
    {
        self.player_position.read().modulo(CHUNK_VISUAL_SIZE)
    }

    fn player_height(&self) -> usize
    {
        self.player_position.read().to_tile().z
    }

    pub fn height(&self, pos: LocalPos) -> usize
    {
        self.maybe_height(pos).unwrap_or(CHUNK_SIZE - 1)
    }

    pub fn maybe_height(&self, pos: LocalPos) -> Option<usize>
    {
        let middle = self.size.z / 2;

        if pos.pos.z == middle
        {
            Some(self.player_height())
        } else
        {
            None
        }
    }

    fn top_z(&self) -> usize
    {
        self.size.z / 2
    }

    fn visible_z(
        &self,
        chunks: &ChunksContainer<(Instant, VisualChunk)>,
        pos: LocalPos
    ) -> impl DoubleEndedIterator<Item=LocalPos>
    {
        let top = self.top_z() + 1;
        let positions = pos.with_z_range(0..top);

        let draw_amount = positions.clone().rev().take_while(|pos|
        {
            chunks[*pos].1.draw_next(self.height(*pos))
        }).count() + 1;

        positions.rev().take(draw_amount)
    }
}

pub struct TileReader
{
    group: MaybeGroup<Arc<Chunk>>
}

impl TileReader
{
    pub fn creatable(
        chunks: &ChunksContainer<Option<Arc<Chunk>>>,
        local_pos: LocalPos
    ) -> bool
    {
        let mut missing = false;
        local_pos.maybe_group().map(|position|
        {
            if chunks[position].is_none()
            {
                missing = true;
            }
        });

        !missing
    }

    pub fn new(
        chunks: &ChunksContainer<Option<Arc<Chunk>>>,
        local_pos: LocalPos
    ) -> Self
    {
        let group = local_pos.maybe_group().map(|position|
        {
            chunks[position].clone().unwrap()
        });

        Self{group}
    }

    pub fn tile(&self, pos: ChunkLocal) -> MaybeGroup<Tile>
    {
        pos.maybe_group().remap(|value|
        {
            self.group.this[value]
        }, |direction, value|
        {
            value.map(|pos|
            {
                Some(self.group.this[pos])
            }).unwrap_or_else(||
            {
                self.group[direction].as_ref().map(|chunk|
                {
                    chunk[pos.overflow(direction)]
                })
            })
        })
    }
}

fn for_visible_2d<'a>(
    chunks: &ChunksContainer<(Instant, VisualChunk)>,
    visibility: &'a VisibilityChecker
) -> impl Iterator<Item=LocalPos> + use<'a>
{
    chunks.positions_2d().filter(|pos| visibility.visible(*pos))
}

#[derive(Debug, Clone)]
struct OccludedSlice
{
    occlusions: [bool; CHUNK_SIZE * CHUNK_SIZE],
    visible_points: Vec<usize>
}

impl OccludedSlice
{
    pub fn empty() -> Self
    {
        let occlusions = [false; CHUNK_SIZE * CHUNK_SIZE];
        let visible_points = (0..occlusions.len()).collect();

        Self{
            occlusions,
            visible_points
        }
    }

    pub fn clear(&mut self)
    {
        *self = Self::empty();
    }

    pub fn is_fully_occluded(&self) -> bool
    {
        self.visible_points.is_empty()
    }

    pub fn occluded(&self, top_left: Vector2<usize>, bottom_right: Vector2<usize>) -> bool
    {
        if self.is_fully_occluded()
        {
            return true;
        }

        (top_left.y..=bottom_right.y).all(|y|
        {
            let index = y * CHUNK_SIZE;
            (top_left.x..=bottom_right.x).all(|x|
            {
                let index = index + x;

                self.occlusions[index]
            })
        })
    }

    fn for_visible_points(&mut self, chunk_pos: Vector2<f32>, occludes: impl Fn(Vector2<f32>) -> bool)
    {
        self.visible_points.retain(|&index|
        {
            let at = |x, y|
            {
                let point = Vector2::new(x, y);

                occludes(point.cast() * TILE_SIZE + chunk_pos)
            };

            let x = index % CHUNK_SIZE;
            let y = index / CHUNK_SIZE;

            let occluded = at(x, y) && at(x + 1, y) && at(x, y + 1) && at(x + 1, y + 1);

            if occluded
            {
                self.occlusions[index] = true;
            }

            !occluded
        })
    }

    pub fn screen_visible_update(
        &mut self,
        visibility: &EntityVisibilityChecker,
        chunk_pos: Vector2<f32>
    )
    {
        self.for_visible_points(chunk_pos, |point|
        {
            !visibility.visible_point_2d(point)
        })
    }

    pub fn update(
        &mut self,
        occluder: &OccluderVisibilityChecker,
        chunk_pos: Vector2<f32>
    )
    {
        self.for_visible_points(chunk_pos, |point|
        {
            occluder.occludes_point_with_epsilon(point, -TILE_SIZE * 0.01)
        })
    }
}

struct GlobalMapper
{
    size: Pos3<usize>,
    position: GlobalPos
}

impl CommonIndexing for GlobalMapper
{
    fn size(&self) -> Pos3<usize>
    {
        self.size
    }
}

impl OvermapIndexing for GlobalMapper
{
    fn player_position(&self) -> GlobalPos
    {
        self.position
    }
}

pub struct VisualOvermap
{
    tiles_factory: TilesFactory,
    chunks: ChunksContainer<(Instant, VisualChunk)>,
    occluded: ChunksContainer<[OccludedSlice; CHUNK_SIZE]>,
    visibility_checker: VisibilityChecker,
    generate_thread: Option<JoinHandle<()>>,
    receiver: Receiver<VisualGenerated>,
    generate_sender: Option<Sender<(TileReader, GlobalPos, Instant)>>
}

impl Drop for VisualOvermap
{
    fn drop(&mut self)
    {
        self.generate_sender.take();
        self.generate_thread.take().unwrap().join().unwrap();
    }
}

impl VisualOvermap
{
    pub fn new(
        tiles_factory: TilesFactory,
        size: Pos3<usize>,
        camera_size: Vector2<f32>,
        player_position: Pos3<f32>
    ) -> Self
    {
        let visibility_checker = VisibilityChecker::new(size, camera_size, player_position);

        let chunks = ChunksContainer::new_with(size, |_| (Instant::now(), VisualChunk::new()));
        let occluded = ChunksContainer::new_with(size, |_|
        {
            iter::repeat_n(OccludedSlice::empty(), CHUNK_SIZE)
                .collect::<Vec<OccludedSlice>>().try_into().unwrap()
        });

        let (sender, receiver) = mpsc::channel();
        let (generate_sender, generate_receiver) = mpsc::channel();

        let (info_map, model_builder) = (tiles_factory.tilemap().clone(), tiles_factory.builder());

        let generate_thread = thread::spawn(move ||
        {
            while let Ok((tile_reader, chunk_pos, timestamp)) = generate_receiver.recv()
            {
                let chunk_info = VisualChunk::create(
                    info_map.clone(),
                    model_builder.clone(),
                    chunk_pos,
                    tile_reader
                );

                let generated = VisualGenerated{
                    chunk_info,
                    position: chunk_pos,
                    timestamp
                };

                sender.send(generated).unwrap();
            }
        });

        Self{
            tiles_factory,
            chunks,
            occluded,
            visibility_checker,
            generate_thread: Some(generate_thread),
            receiver,
            generate_sender: Some(generate_sender)
        }
    }

    pub fn try_generate(
        &mut self,
        chunks: &ChunksContainer<Option<Arc<Chunk>>>,
        pos: LocalPos
    )
    {
        if self.is_generated(pos)
        {
            return;
        }

        self.force_generate(chunks, pos);
    }

    pub fn try_force_generate(
        &mut self,
        chunks: &ChunksContainer<Option<Arc<Chunk>>>,
        pos: LocalPos
    )
    {
        if chunks[pos].is_none()
        {
            return;
        }

        if !TileReader::creatable(chunks, pos)
        {
            return;
        }

        self.force_generate(chunks, pos);
    }

    pub fn force_generate(
        &mut self,
        chunks: &ChunksContainer<Option<Arc<Chunk>>>,
        pos: LocalPos
    )
    {
        self.mark_generating(pos);

        let tile_reader = TileReader::new(chunks, pos);

        let chunk_pos = self.to_global(pos);

        self.generate_sender.as_mut().unwrap().send((tile_reader, chunk_pos, Instant::now())).unwrap();
    }

    pub fn update(&mut self, _dt: f32)
    {
        self.process_message();
    }

    pub fn process_message(&mut self)
    {
        if let Ok(generated) = self.receiver.try_recv()
        {
            self.handle_generated(generated);
        }
    }

    fn handle_generated(&mut self, generated: VisualGenerated)
    {
        let VisualGenerated{chunk_info, position, timestamp} = generated;

        if let Some(local_pos) = self.to_local(position)
        {
            let current_chunk = &mut self.chunks[local_pos];

            if current_chunk.0 <= timestamp
            {
                let chunk = VisualChunk::build(&mut self.tiles_factory, chunk_info);

                *current_chunk = (timestamp, chunk);
            }
        }
    }

    pub fn rescale(&mut self, camera_size: Vector2<f32>)
    {
        self.visibility_checker.camera_size = camera_size;
    }

    pub fn camera_moved(&mut self, position: Pos3<f32>)
    {
        *self.visibility_checker.player_position.write() = position;
    }

    pub fn mark_generating(&mut self, pos: LocalPos)
    {
        self.chunks[pos].1.mark_generating();
    }

    pub fn mark_ungenerated(&mut self, pos: LocalPos)
    {
        self.chunks[pos].1.mark_ungenerated();
    }

    #[allow(dead_code)]
    pub fn mark_all_ungenerated(&mut self)
    {
        self.chunks.iter_mut().for_each(|(_, (_, chunk))|
        {
            chunk.mark_ungenerated();
        });
    }

    pub fn get(&self, pos: LocalPos) -> &VisualChunk
    {
        &self.chunks[pos].1
    }

    pub fn is_generated(&self, pos: LocalPos) -> bool
    {
        self.get(pos).is_generated()
    }

    pub fn remove(&mut self, pos: LocalPos)
    {
        self.chunks[pos] = (Instant::now(), VisualChunk::new());
    }

    pub fn swap(&mut self, a: LocalPos, b: LocalPos)
    {
        self.chunks.swap(a, b);
    }

    fn sky_occluders_heights(
        visibility_checker: &VisibilityChecker,
        pos: LocalPos
    ) -> impl Iterator<Item=LocalPos>
    {
        let size_z = visibility_checker.size.z;
        let top = size_z / 2;
        pos.with_z_range(top..size_z)
    }

    fn sky_draw_height(height: Option<usize>) -> usize
    {
        height.map(|x| (x + 1).min(CHUNK_SIZE - 1)).unwrap_or(0)
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            self.visibility_checker.visible_z(&self.chunks, pos).for_each(|pos|
            {
                self.chunks[pos].1.update_buffers(
                    info,
                    self.visibility_checker.height(pos)
                )
            });

            Self::sky_occluders_heights(&self.visibility_checker, pos).for_each(|pos|
            {
                self.chunks[pos].1.update_sky_buffers(
                    info,
                    Self::sky_draw_height(self.visibility_checker.maybe_height(pos))
                );
            });
        });
    }

    pub fn debug_tile_occlusion(&self, entities: &ClientEntities)
    {
        let z = self.visibility_checker.top_z();
        let height = self.visibility_checker.player_height();
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            let pos = pos.with_z(z);

            let chunk_pos = Chunk::position_of_chunk(self.to_global(pos));
            Self::debug_tile_occlusion_single(
                &self.occluded[pos],
                entities,
                chunk_pos,
                height
            );
        });
    }

    fn debug_tile_occlusion_single(
        occluded: &[OccludedSlice; CHUNK_SIZE],
        entities: &ClientEntities,
        chunk_pos: Vector3<f32>,
        height: usize
    )
    {
        occluded[height].occlusions.iter().enumerate().for_each(|(index, state)|
        {
            let x = index % CHUNK_SIZE;
            let y = index / CHUNK_SIZE;

            let tile_position = Vector3::new(
                x as f32,
                y as f32,
                height as f32
            ) * TILE_SIZE + chunk_pos;

            let position = tile_position + Vector3::repeat(TILE_SIZE / 2.0);

            let color = if *state
            {
                [1.0, 0.0, 0.0, 0.2]
            } else
            {
                [0.0, 1.0, 0.0, 0.2]
            };

            /*entities.push(true, EntityInfo{
                transform: Some(Transform{
                    position: tile_position,
                    scale: Vector3::repeat(0.03),
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "circle.png".to_owned()
                    }.into()),
                    above_world: true,
                    mix: Some(MixColor{keep_transparency: true, ..MixColor::color(if occluded[height].points[y * (CHUNK_SIZE + 1) + x] { [0.0, 0.0, 1.0, 1.0] } else { [0.0, 1.0, 0.0, 1.0] })}),
                    ..Default::default()
                }),
                watchers: Some(Watchers::simple_one_frame()),
                ..Default::default()
            });*/

            entities.push(true, EntityInfo{
                transform: Some(Transform{
                    position,
                    scale: Vector3::repeat(TILE_SIZE),
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "ui/solid.png".to_owned()
                    }.into()),
                    above_world: true,
                    mix: Some(MixColor::color(color)),
                    ..Default::default()
                }),
                watchers: Some(Watchers::simple_one_frame()),
                ..Default::default()
            });
        });
    }

    pub fn draw_tiles(
        &self,
        info: &mut DrawInfo,
        is_shaded: bool
    )
    {
        let z = self.visibility_checker.top_z();
        let player_height = self.visibility_checker.player_height();
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            if !is_shaded && self.occluded[pos.with_z(z)][player_height].is_fully_occluded()
            {
                return;
            }

            self.visibility_checker.visible_z(&self.chunks, pos).rev().for_each(|pos|
            {
                self.chunks[pos].1.draw_tiles(
                    info,
                    self.visibility_checker.height(pos)
                )
            });
        });
    }

    fn global_mapper(&self) -> GlobalMapper
    {
        GlobalMapper{
            size: self.size(),
            position: self.player_position()
        }
    }

    fn clear_occluders(&mut self, visibility: &EntityVisibilityChecker)
    {
        let mapper = self.global_mapper();

        let z = self.visibility_checker.top_z();
        let height = self.visibility_checker.player_height();

        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            let pos = pos.with_z(z);

            let occluded = &mut self.occluded[pos][height];

            occluded.clear();

            let chunk_pos = Chunk::position_of_chunk(mapper.to_global(pos));

            occluded.screen_visible_update(visibility, chunk_pos.xy());
        });
    }

    pub fn update_buffers_shadows(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &EntityVisibilityChecker,
        caster: &OccludingCaster
    )
    {
        self.clear_occluders(visibility);

        let size = self.chunks.size();

        let z = self.visibility_checker.top_z();

        let mapper = self.global_mapper();

        let mut visible_occluders = Vec::new();

        let player_position = self.visibility_checker.player_position.read();
        let height = self.visibility_checker.player_height();
        size.positions_2d().for_each(|pos|
        {
            if !self.visibility_checker.visible(pos)
            {
                return;
            }

            let pos = pos.with_z(z);

            self.chunks[pos].1.update_buffers_shadows(
                info,
                visibility,
                caster,
                height,
                |OccluderCached{occluder, indices, ..}, index|
                {
                    visible_occluders.push((occluder.occluder_visibility_checker().unwrap(), *indices, index, pos));
                }
            )
        });

        visible_occluders.sort_unstable_by_key(|(occluder, _, _, _)|
        {
            let distance = occluder.front_position().metric_distance(&Vector3::from(*player_position).xy());
            SortableF32::from(distance)
        });

        visible_occluders.into_iter().for_each(|(occluder, indices, occluder_index, pos)|
        {
            {
                let current_occluded = &self.occluded[pos][height];
                if indices.iter().all(|index| current_occluded.occlusions[index])
                {
                    self.chunks[pos].1.set_occluder_visible(height, occluder_index, false);
                    return;
                }
            }

            self.chunks[pos].1.set_occluder_visible(height, occluder_index, true);

            size.positions_2d().for_each(|check_pos|
            {
                if !self.visibility_checker.visible(check_pos)
                {
                    return;
                }

                let check_pos = check_pos.with_z(z);

                let chunk_pos = Chunk::position_of_chunk(mapper.to_global(check_pos));
                self.occluded[check_pos][height].update(&occluder, chunk_pos.xy());
            });
        });
    }

    pub fn draw_shadows(
        &self,
        info: &mut DrawInfo
    )
    {
        let z = self.visibility_checker.top_z();
        let height = self.visibility_checker.player_height();
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            let pos = pos.with_z(z);

            self.chunks[pos].1.draw_shadows(
                info,
                height
            );
        });
    }

    fn chunk_height_of(
        size_z: usize,
        position: f32,
        player_position: i32
    ) -> Option<usize>
    {
        let size_z = size_z as i32;

        let chunk_height = Pos3::repeat(position).rounded().0.z - player_position + (size_z / 2);

        if !(0..size_z).contains(&chunk_height)
        {
            return None;
        }

        Some(chunk_height as usize)
    }

    fn with_position(
        mut pos: LocalPos,
        position: f32,
        player_position: i32
    ) -> Option<(LocalPos, usize)>
    {
        pos.pos.z = Self::chunk_height_of(pos.size.z, position, player_position)?;

        let position = Pos3::repeat(position);
        let height = position.to_tile().z;

        Some((pos, height))
    }

    pub fn sky_occluded(&self, transform: &Transform, e: Option<&ClientEntities>) -> bool
    {
        let size_z = self.visibility_checker.size.z;
        let player_position_z = self.visibility_checker.player_position.read().rounded().0.z;
        let player_height = self.visibility_checker.player_height();

        let z = Self::chunk_height_of(size_z, transform.position.z, player_position_z);
        self.occluded_with(transform, e, |pos, height, top_left, bottom_right|
        {
            let (z, height) = if let Some(z) = z { (z, height) } else { (0, 0) };

            let camera_z = size_z / 2;
            pos.with_z_range(z..(camera_z + 1)).enumerate().any(|(index, pos)|
            {
                let height = if index == 0 { height } else { 0 };

                let chunk_pos = Chunk::position_of_chunk(self.to_global(pos));

                let chunk = &self.chunks[pos].1;

                if pos.pos.z == camera_z
                {
                    chunk.sky_occluded_between(height..=player_height, top_left, bottom_right, (e, chunk_pos))
                } else
                {
                    chunk.sky_occluded(height, top_left, bottom_right, (e, chunk_pos))
                }
            })
        })
    }

    pub fn wall_occluded(&self, transform: &Transform) -> bool
    {
        self.occluded_with(transform, None, |pos, height, top_left, bottom_right|
        {
            self.occluded[pos][height].occluded(top_left, bottom_right)
        })
    }

    fn occluded_with(
        &self,
        transform: &Transform,
        e: Option<&ClientEntities>,
        f: impl Fn(LocalPos, usize, Vector2<usize>, Vector2<usize>) -> bool
    ) -> bool
    {
        let pos = transform.position;
        let size = transform.scale * 0.5;
        let size = Vector3::new(size.x.abs(), size.y.abs(), 0.0);

        let (top_left_pos, bottom_right_pos) = if transform.rotation == 0.0
        {
            (pos - size, pos + size)
        } else
        {
            let (a, b) = aabb_points(transform);

            (Vector3::new(a.x, a.y, pos.z), Vector3::new(b.x, b.y, pos.z))
        };

        if let Some(e) = e
        {
            use crate::common::AnyEntities;
            e.push(true, EntityInfo{
                transform: Some(Transform{
                    position: top_left_pos,
                    scale: Vector3::repeat(0.01),
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "circle.png".to_owned()
                    }.into()),
                    above_world: true,
                    mix: Some(MixColor{keep_transparency: true, ..MixColor::color([0.0, 0.0, 1.0, 1.0])}),
                    ..Default::default()
                }),
                watchers: Some(Watchers::simple_one_frame()),
                ..Default::default()
            });
            e.push(true, EntityInfo{
                transform: Some(Transform{
                    position: bottom_right_pos,
                    scale: Vector3::repeat(0.01),
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "circle.png".to_owned()
                    }.into()),
                    above_world: true,
                    mix: Some(MixColor{keep_transparency: true, ..MixColor::color([1.0, 1.0, 0.0, 1.0])}),
                    ..Default::default()
                }),
                watchers: Some(Watchers::simple_one_frame()),
                ..Default::default()
            });
        }

        let (top_left, top_left_tile) = {
            let pos: Pos3<_> = top_left_pos.into();

            let chunk = self.to_local(pos.rounded()).unwrap_or_else(||
            {
                LocalPos::new(Pos3::repeat(0), self.visibility_checker.size)
            });

            let tile = pos.to_tile();

            (chunk, tile)
        };

        let (bottom_right, bottom_right_tile) = {
            let pos: Pos3<_> = bottom_right_pos.into();

            let chunk = self.to_local(pos.rounded()).unwrap_or_else(||
            {
                LocalPos::new(self.visibility_checker.size - Pos3::repeat(1), self.visibility_checker.size)
            });

            let tile = pos.to_tile();

            (chunk, tile)
        };

        (top_left.pos.y..=bottom_right.pos.y).all(|y|
        {
            let f = &f;
            (top_left.pos.x..=bottom_right.pos.x).all(move |x|
            {
                let pos = LocalPos::new(Pos3{x, y, z: top_left.pos.z}, top_left.size);

                let tile_start = Vector2::new(
                    if x == top_left.pos.x { top_left_tile.x } else { 0 },
                    if y == top_left.pos.y { top_left_tile.y } else { 0 }
                );

                let tile_end = Vector2::new(
                    if x == bottom_right.pos.x { bottom_right_tile.x } else { CHUNK_SIZE - 1 },
                    if y == bottom_right.pos.y { bottom_right_tile.y } else { CHUNK_SIZE - 1 }
                );

                f(pos, top_left_tile.z, tile_start, tile_end)
            })
        })
    }

    pub fn update_buffers_light_shadows(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &EntityVisibilityChecker,
        caster: &OccludingCaster,
        id: usize
    )
    {
        let player_position = self.visibility_checker.player_position.read().rounded().0.z;
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            if let Some((pos, height)) = Self::with_position(pos, visibility.position.z, player_position)
            {
                self.chunks[pos].1.update_buffers_light_shadows(
                    info,
                    &mut self.tiles_factory,
                    visibility,
                    caster,
                    height,
                    id
                );
            }
        });
    }

    pub fn draw_light_shadows(
        &self,
        info: &mut DrawInfo,
        visibility: &EntityVisibilityChecker,
        id: usize,
        f: impl FnOnce(&mut DrawInfo)
    )
    {
        let mut f = Some(f);
        let player_position = self.visibility_checker.player_position.read().rounded().0.z;
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            let (pos, height) = Self::with_position(pos, visibility.position.z, player_position).unwrap();

            self.chunks[pos].1.draw_light_shadows(
                info,
                height,
                id,
                &mut f
            );
        });
    }

    pub fn draw_sky_occluders(
        &self,
        info: &mut DrawInfo
    )
    {
        let z = self.visibility_checker.top_z();
        let player_height = self.visibility_checker.player_height();
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            if self.occluded[pos.with_z(z)][player_height].is_fully_occluded()
            {
                return;
            }

            Self::sky_occluders_heights(&self.visibility_checker, pos).for_each(|pos|
            {
                self.chunks[pos].1.draw_sky_shadows(
                    info,
                    Self::sky_draw_height(self.visibility_checker.maybe_height(pos))
                );
            });
        });
    }
}

impl CommonIndexing for VisualOvermap
{
    fn size(&self) -> Pos3<usize>
    {
        self.visibility_checker.size
    }
}

impl OvermapIndexing for VisualOvermap
{
    fn player_position(&self) -> GlobalPos
    {
        self.visibility_checker.player_position.read().rounded()
    }
}
