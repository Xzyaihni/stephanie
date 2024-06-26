use std::{
    thread,
    marker::PhantomData,
    cmp::Ordering,
    io::{self, Write, Read, Seek, SeekFrom},
    time::{Instant, Duration},
    path::{Path, PathBuf},
    fs::{self, OpenOptions, File},
    collections::{HashMap, BinaryHeap},
    sync::{
        Arc,
        mpsc::{self, Sender, Receiver}
    }
};

use parking_lot::Mutex;

use lzma::{LzmaWriter, LzmaReader};

use serde::{Serialize, Deserialize};

use crate::{
    server::world::world_generator::{CHUNK_RATIO, MaybeWorldChunk, WorldChunk, WorldChunkTag},
    common::{
        EntityInfo,
        world::{
            Chunk,
            GlobalPos,
            Pos3
        }
    }
};


// goes from 0 to 9, 0 being lowest level of compression
const LZMA_PRESET: u32 = 1;
const SAVE_MODULO: u32 = 20;

pub trait Saveable: Send + 'static {}
pub trait AutoSaveable: Saveable {}

impl Saveable for Chunk {}
impl Saveable for Vec<EntityInfo> {}
impl Saveable for SaveValueGroup {}

impl AutoSaveable for Chunk {}
impl AutoSaveable for Vec<EntityInfo> {}

pub trait SaveLoad<T>
{
    fn save(&mut self, pos: GlobalPos, chunk: T);
    fn load(&mut self, pos: GlobalPos) -> Option<T>;
}

// this shouldnt be public but sure, rust
pub trait FileSave
{
    type SaveItem;
    type LoadItem;

    fn new(parent_path: PathBuf) -> Self;

    fn save(&mut self, pair: ValuePair<Self::SaveItem>);
    fn load(&mut self, pos: GlobalPos) -> Option<Self::LoadItem>;
}

// again, shouldnt be public
#[derive(Debug)]
pub struct SaveValueGroup
{
    value: WorldChunk,
    index: usize
}

impl SaveValueGroup
{
    pub fn write_into<W>(self, mut writer: W)
    where
        W: Write + Seek
    {
        let start = MaybeWorldChunk::index_of(self.index);

        writer.seek(SeekFrom::Start(start as u64)).unwrap();

        MaybeWorldChunk::from(self.value).write_into(writer)
    }
}

// again, shouldnt be public
#[derive(Debug)]
pub struct LoadValueGroup
{
    file: File,
    tags_file: Option<File>
}

impl LoadValueGroup
{
    pub fn get(&mut self, index: usize) -> Option<WorldChunk>
    {
        let size = MaybeWorldChunk::size_of();

        let start = MaybeWorldChunk::index_of(index);

        self.file.seek(SeekFrom::Start(start as u64)).unwrap();

        let mut bytes = Vec::with_capacity(size);
        <File as Read>::by_ref(&mut self.file)
            .take(size as u64)
            .read_to_end(&mut bytes)
            .unwrap();

        let world_chunk: Option<_> = MaybeWorldChunk::from_bytes(&bytes).into();

        world_chunk.map(|world_chunk: WorldChunk|
        {
            let tags = self.tags_file.as_mut().map(|file|
            {
                bincode::deserialize_from(file).unwrap()
            }).unwrap_or_default();

            world_chunk.with_tags(tags)
        })
    }
}

pub struct ValuePair<T>
{
    pub key: GlobalPos,
    pub value: T
}

#[derive(Debug)]
pub struct CachedKey
{
    age: Duration,
    pub pos: GlobalPos
}

impl Eq for CachedKey
{
}

impl PartialOrd for CachedKey
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering>
    {
        Some(self.cmp(other))
    }
}

impl Ord for CachedKey
{
    fn cmp(&self, other: &Self) -> Ordering
    {
        self.age.cmp(&other.age)
    }
}

impl PartialEq for CachedKey
{
    fn eq(&self, other: &Self) -> bool
    {
        self.age.eq(&other.age)
    }
}

impl CachedKey
{
    pub fn new(start: Instant, pos: GlobalPos) -> Self
    {
        Self{age: start.elapsed(), pos}
    }
}

#[derive(Debug)]
pub struct CachedValue<T>
{
    pub key: CachedKey,
    pub value: T
}

impl<T> Eq for CachedValue<T>
{
}

impl<T> PartialOrd for CachedValue<T>
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering>
    {
        self.key.partial_cmp(&other.key)
    }
}

impl<T> Ord for CachedValue<T>
{
    fn cmp(&self, other: &Self) -> Ordering
    {
        other.key.cmp(&self.key)
    }
}

impl<T> PartialEq for CachedValue<T>
{
    fn eq(&self, other: &Self) -> bool
    {
        self.key.eq(&other.key)
    }
}

struct BlockingSaver<T>
{
    parent_path: PathBuf,
    save_rx: Receiver<ValuePair<T>>,
    finish_tx: Sender<GlobalPos>
}

impl<T> BlockingSaver<T>
{
    pub fn new(
        parent_path: PathBuf,
        save_rx: Receiver<ValuePair<T>>,
        finish_tx: Sender<GlobalPos>
    ) -> Self
    {
        Self{parent_path, save_rx, finish_tx}
    }

    fn parent_path(&self, pos: GlobalPos) -> PathBuf
    {
        Self::parent_path_assoc(&self.parent_path, pos)
    }

    fn parent_path_assoc(parent_path: &Path, pos: GlobalPos) -> PathBuf
    {
        let pos_modulo = pos.0.map(|value| value / SAVE_MODULO as i32);

        parent_path
            .join(pos_modulo.z.to_string())
            .join(pos_modulo.y.to_string())
            .join(pos_modulo.x.to_string())
    }

    fn encode_position(pos: GlobalPos) -> String
    {
        let GlobalPos(Pos3{x, y, z}) = pos;

        format!("{x}_{y}_{z}")
    }
}

impl<T> BlockingSaver<T>
{
    pub fn run<F>(self, mut save_fn: F)
    where
        F: FnMut(PathBuf, ValuePair<T>)
    {
        while let Ok(pair) = self.save_rx.recv()
        {
            let pos = pair.key;
            let path = self.parent_path(pos);

            fs::create_dir_all(&path).unwrap();

            save_fn(path, pair);

            self.finish_tx.send(pos).unwrap();
        }
    }
}

// again, shouldnt be public
#[derive(Debug)]
pub struct FileSaver<SaveT, LoadT=SaveT>
{
    parent_path: PathBuf,
    // i need the usize field just to count the saves called for the same chunk
    unsaved_chunks: HashMap<GlobalPos, usize>,
    save_tx: Sender<ValuePair<SaveT>>,
    finish_rx: Receiver<GlobalPos>,
    phantom: PhantomData<(SaveT, LoadT)>
}

impl<SaveT: Saveable, LoadT> FileSaver<SaveT, LoadT>
{
    fn new_with_saver<F>(parent_path: PathBuf, save_fn: F) -> Self
    where
        F: FnMut(PathBuf, ValuePair<SaveT>) + Send + 'static
    {
        let (save_tx, save_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();

        {
            let parent_path = parent_path.clone();

            thread::spawn(||
            {
                let saver: BlockingSaver<SaveT> = BlockingSaver::new(
                    parent_path,
                    save_rx,
                    finish_tx
                );

                saver.run(save_fn);
            });
        }

        Self{
            parent_path,
            unsaved_chunks: HashMap::new(),
            save_tx,
            finish_rx,
            phantom: PhantomData
        }
    }

    fn block_until(&mut self, pos: GlobalPos)
    {
        while let Ok(finished_pos) = self.finish_rx.recv()
        {
            let count = self.unsaved_chunks.get_mut(&finished_pos).unwrap();
            *count -= 1;

            if *count == 0
            {
                self.unsaved_chunks.remove(&finished_pos);

                if finished_pos == pos
                {
                    return;
                }
            }
        }
    }

    fn is_unsaved(&self, pos: GlobalPos) -> bool
    {
        self.unsaved_chunks.contains_key(&pos)
    }

    fn load_with<F>(&mut self, pos: GlobalPos, load_fn: F) -> Option<LoadT>
    where
        F: FnOnce(PathBuf, File) -> LoadT
    {
        if self.is_unsaved(pos)
        {
            self.block_until(pos);
        }

        let parent_path = self.parent_path(pos);

        match File::open(Self::chunk_path(parent_path.clone(), pos))
        {
            Ok(file) =>
            {
                Some(load_fn(parent_path, file))
            },
            Err(ref err) if err.kind() == io::ErrorKind::NotFound =>
            {
                None
            },
            Err(err) => panic!("error loading chunk from file: {err}")
        }
    }

    fn save_inner(&mut self, pair: ValuePair<SaveT>)
    {
        let entry = self.unsaved_chunks.entry(pair.key).or_insert(0);
        *entry += 1;

        self.save_tx.send(pair).unwrap();
    }

    fn parent_path(&self, pos: GlobalPos) -> PathBuf
    {
        BlockingSaver::<SaveT>::parent_path_assoc(&self.parent_path, pos)
    }

    fn encode_position(pos: GlobalPos) -> String
    {
        BlockingSaver::<SaveT>::encode_position(pos)
    }

    fn chunk_path(parent_path: PathBuf, pos: GlobalPos) -> PathBuf
    {
        parent_path.join(Self::encode_position(pos))
    }

    // i keep making these functions, i feel silly
    fn tags_parent_path(parent_path: PathBuf) -> PathBuf
    {
        parent_path.join("tags")
    }

    fn save_tags(parent_path: PathBuf, pos: GlobalPos, tags: &[WorldChunkTag])
    {
        if tags.is_empty()
        {
            return;
        }

        let parent_path = Self::tags_parent_path(parent_path);

        match fs::create_dir(&parent_path)
        {
            Ok(_) => (),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
            Err(err) => panic!("{err}")
        }

        let file = File::create(Self::chunk_path(parent_path, pos)).unwrap();

        bincode::serialize_into(file, tags).unwrap();
    }
}

impl<T> FileSave for FileSaver<T>
where
    for<'a> T: Saveable + Deserialize<'a> + Serialize
{
    type SaveItem = T;
    type LoadItem = T;

    fn new(parent_path: PathBuf) -> Self
    {
        Self::new_with_saver(parent_path, |path, pair|
        {
            let file = File::create(Self::chunk_path(path, pair.key)).unwrap();

            let mut lzma_writer = LzmaWriter::new_compressor(file, LZMA_PRESET).unwrap();

            bincode::serialize_into(&mut lzma_writer, &pair.value).unwrap();

            lzma_writer.finish().unwrap();
        })
    }

    fn save(&mut self, pair: ValuePair<Self::SaveItem>)
    {
        self.save_inner(pair);
    }

    fn load(&mut self, pos: GlobalPos) -> Option<Self::LoadItem>
    {
        self.load_with(pos, |_parent_path, file|
        {
            let lzma_reader = LzmaReader::new_decompressor(file).unwrap();

            bincode::deserialize_from(lzma_reader).unwrap()
        })
    }
}

impl FileSave for FileSaver<SaveValueGroup, LoadValueGroup>
{
    type SaveItem = SaveValueGroup;
    type LoadItem = LoadValueGroup;

    fn new(parent_path: PathBuf) -> Self
    {
        Self::new_with_saver(parent_path, |path, pair|
        {
            let chunk_path = Self::chunk_path(path.clone(), pair.key);
            let file = match OpenOptions::new().write(true).open(&chunk_path)
            {
                Ok(file) => file,
                Err(ref err) if err.kind() == io::ErrorKind::NotFound =>
                {
                    let mut file = File::create(chunk_path).unwrap();

                    (0..CHUNK_RATIO.product()).for_each(|_|
                    {
                        MaybeWorldChunk::default().write_into(&mut file);
                    });

                    file
                },
                Err(err) => panic!("error loading worldchunk from file: {err}")
            };

            let mut value = pair.value;
            let tags = value.value.take_tags();

            Self::save_tags(path, pair.key, &tags);

            value.write_into(file);
        })
    }

    fn save(&mut self, pair: ValuePair<Self::SaveItem>)
    {
        self.save_inner(pair);
    }

    fn load(&mut self, pos: GlobalPos) -> Option<Self::LoadItem>
    {
        self.load_with(pos, |parent_path, file|
        {
            let chunk_path = Self::chunk_path(Self::tags_parent_path(parent_path), pos);

            let tags_file = match File::open(chunk_path)
            {
                Ok(file) => Some(file),
                Err(ref err) if err.kind() == io::ErrorKind::NotFound =>
                {
                    None
                },
                Err(err) => panic!("error loading tags from file: {err}")
            };

            LoadValueGroup{file, tags_file}
        })
    }
}

pub type ChunkSaver = Saver<FileSaver<Chunk>, Chunk>;
pub type WorldChunkSaver = Saver<FileSaver<SaveValueGroup, LoadValueGroup>, SaveValueGroup, LoadValueGroup>;

pub struct EntitiesSaver
{
    saver: Saver<FileSaver<Vec<EntityInfo>>, Vec<EntityInfo>>
}

// again, shouldnt be public
#[derive(Debug)]
pub struct Saver<S, SaveT: Saveable, LoadT=SaveT>
where
    S: FileSave<SaveItem=SaveT, LoadItem=LoadT>
{
    start: Instant,
    cache_amount: usize,
    cache: BinaryHeap<CachedValue<SaveT>>,
    file_saver: Arc<Mutex<S>>
}

impl<S, SaveT: Saveable, LoadT> Saver<S, SaveT, LoadT>
where
    S: FileSave<SaveItem=SaveT, LoadItem=LoadT>
{
    pub fn new(parent_path: impl Into<PathBuf>, cache_amount: usize) -> Self
    {
        let parent_path = parent_path.into();

        fs::create_dir_all(&parent_path).unwrap();

        let file_saver = S::new(parent_path);

        Self{
            start: Instant::now(),
            file_saver: Arc::new(Mutex::new(file_saver)),
            cache_amount,
            cache: BinaryHeap::new()
        }
    }

    fn free_cache(&mut self, amount: usize)
    {
        let until_len = self.cache_amount.saturating_sub(amount);

        while self.cache.len() > until_len
        {
            let CachedValue{key: CachedKey{pos: key, ..}, value} = self.cache.pop().unwrap();

            self.file_saver.lock().save(ValuePair{key, value});
        }
    }

    fn inner_save(&mut self, pair: ValuePair<SaveT>)
    {
        let key = CachedKey::new(self.start, pair.key);

        if self.cache.iter().any(|CachedValue{key, ..}| key.pos == pair.key)
        {
            self.cache.retain(|CachedValue{key, ..}| key.pos != pair.key);
        } else
        {
            self.free_cache(1);
        }

        self.cache.push(CachedValue{key, value: pair.value});
    }
}

impl SaveLoad<WorldChunk> for WorldChunkSaver
{
    fn load(&mut self, pos: GlobalPos) -> Option<WorldChunk>
    {
        let index = WorldChunk::global_to_index(pos);

        let rounded_pos = WorldChunk::belongs_to(pos);

        if let Some(CachedValue{
            value: found,
            ..
        }) = self.cache.iter().find(|CachedValue{key, value}|
        {
            (key.pos == rounded_pos) && (value.index == index)
        })
        {
            return Some(found.value.clone());
        }

        self.file_saver.lock().load(rounded_pos)
            .and_then(|mut load_chunk| load_chunk.get(index))
    }

    fn save(&mut self, pos: GlobalPos, chunk: WorldChunk)
    {
        let index = WorldChunk::global_to_index(pos);

        let value = SaveValueGroup{value: chunk, index};
        let pos = WorldChunk::belongs_to(pos);

        let key = CachedKey::new(self.start, pos);

        self.free_cache(1);
        self.cache.push(CachedValue{key, value});
    }
}

impl EntitiesSaver
{
    pub fn new(parent_path: impl Into<PathBuf>, cache_amount: usize) -> Self
    {
        Self{
            saver: Saver::new(parent_path, cache_amount)
        }
    }

    pub fn load(&mut self, pos: GlobalPos) -> Option<Vec<EntityInfo>>
    {
        let loaded = self.saver.load(pos);

        if loaded.is_some()
        {
            self.saver.save(pos, Vec::new());
        }

        loaded
    }

    pub fn save(&mut self, pos: GlobalPos, mut entities: Vec<EntityInfo>)
    {
        let entities = if let Some(mut contained) = self.saver.load(pos)
        {
            contained.append(&mut entities);

            contained
        } else
        {
            entities
        };

        self.saver.save(pos, entities);
    }
}

impl<T> SaveLoad<T> for Saver<FileSaver<T>, T>
where
    T: AutoSaveable + Clone,
    FileSaver<T>: FileSave<LoadItem=T, SaveItem=T>
{
    fn load(&mut self, pos: GlobalPos) -> Option<T>
    {
        if let Some(CachedValue{
            value: found,
            ..
        }) = self.cache.iter().find(|CachedValue{key, ..}|
        {
            key.pos == pos
        })
        {
            return Some(found.clone());
        }

        self.file_saver.lock().load(pos)
    }

    fn save(&mut self, pos: GlobalPos, value: T)
    {
        let pair = ValuePair{key: pos, value};

        self.inner_save(pair);
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    use crate::server::world::world_generator::WorldChunkId;

    use std::iter;


    #[ignore]
    #[test]
    fn world_chunk_saving()
    {
        let clear_dir = |dir_name: &Path|
        {
            if dir_name.exists()
            {
                fs::read_dir(dir_name).unwrap();
            }
        };

        let dir_name = PathBuf::from("test_world");

        clear_dir(&dir_name);

        // random ass numbers
        let size = Pos3::new(
            fastrand::usize(4..7),
            fastrand::usize(4..7),
            fastrand::usize(40..70)
        );

        let random_worldchunk = ||
        {
            WorldChunk::new(WorldChunkId::from_raw(fastrand::usize(0..100)), Vec::new())
        };

        let mut chunks: Vec<_> = iter::repeat_with(||
            {
                random_worldchunk()
            }).zip((0..).map(|index|
            {
                let x = index % size.x;
                let y = (index / size.x) % size.y;
                let z = index / (size.x * size.y);

                GlobalPos::from(Pos3::new(x, y, z))
            })).take(size.product() - 10 + fastrand::usize(0..20))
            .collect();

        let mut saver = WorldChunkSaver::new(&dir_name, 4);

        for (chunk, pos) in chunks.iter().cloned()
        {
            saver.save(pos, chunk);
        }

        for i in 1..10
        {
            saver.save(GlobalPos::from(size + i), random_worldchunk());
        }

        let mut shuffled_chunks = Vec::with_capacity(chunks.len());

        while !chunks.is_empty()
        {
            let element = chunks.swap_remove(fastrand::usize(0..chunks.len()));

            shuffled_chunks.push(element);
        }

        for (chunk, pos) in shuffled_chunks
        {
            assert_eq!(Some(chunk), saver.load(pos));
        }

        clear_dir(&dir_name);
    }
}
