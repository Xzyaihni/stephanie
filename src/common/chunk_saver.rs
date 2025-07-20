use std::{
    any,
    thread,
    fmt::Debug,
    cmp::Ordering,
    io,
    time::{Instant, Duration},
    path::{Path, PathBuf},
    fs::{self, File},
    collections::{HashMap, BinaryHeap},
    sync::{
        Arc,
        mpsc::{self, Sender, Receiver}
    }
};

use parking_lot::Mutex;

use serde::{Serialize, Deserialize};

use crate::{
    server::world::world_generator::{CHUNK_RATIO, WorldChunk},
    common::{
        FullEntityInfo,
        world::{
            Chunk,
            GlobalPos,
            Pos3
        }
    }
};

pub use saver::*;

mod saver;


const SAVE_MODULO: u32 = 20;

pub trait Saveable: Debug + Clone + Send + 'static {}
pub trait AutoSaveable: Saveable {}

pub type WorldChunksBlock = [WorldChunk; CHUNK_RATIO.z];

impl Saveable for Chunk {}
impl Saveable for SaveEntities {}
impl Saveable for WorldChunksBlock {}

impl AutoSaveable for Chunk {}
impl AutoSaveable for SaveEntities {}
impl AutoSaveable for WorldChunksBlock {}

pub trait SaveLoad<T>
{
    fn save(&mut self, pos: GlobalPos, chunk: T);
    fn load(&mut self, pos: GlobalPos) -> Option<T>;
}

// this shouldnt be public but sure, rust
pub trait FileSave
{
    type SaveItem;

    fn new(parent_path: PathBuf) -> Self;

    fn save(&mut self, pair: ValuePair<Self::SaveItem>);
    fn load(&mut self, pos: GlobalPos) -> Option<Self::SaveItem>;

    fn flush(&mut self);
}

#[derive(Debug, Clone)]
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

impl<T> From<CachedValue<T>> for ValuePair<T>
{
    fn from(value: CachedValue<T>) -> Self
    {
        let CachedValue{
            key: CachedKey{pos: key, ..},
            value
        } = value;

        ValuePair{key, value}
    }
}

impl<T> Eq for CachedValue<T>
{
}

impl<T> PartialOrd for CachedValue<T>
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering>
    {
        Some(self.key.cmp(&other.key))
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
pub struct FileSaver<SaveT: Saveable>
{
    parent_path: PathBuf,
    // i need the usize field just to count the saves called for the same chunk
    unsaved_chunks: HashMap<GlobalPos, usize>,
    save_tx: Sender<ValuePair<SaveT>>,
    finish_rx: Receiver<GlobalPos>
}

impl<SaveT: Saveable> Drop for FileSaver<SaveT>
{
    fn drop(&mut self)
    {
        self.flush();
    }
}

impl<SaveT: Saveable> FileSaver<SaveT>
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
            finish_rx
        }
    }

    #[allow(dead_code)]
    fn has_unsaved(&self) -> bool
    {
        !self.unsaved_chunks.is_empty()
    }

    fn flush(&mut self)
    {
        self.block_until_with(|_| false);
    }

    fn block_until(&mut self, pos: GlobalPos)
    {
        self.block_until_with(|finished_pos| finished_pos == pos);
    }

    fn block_until_with(&mut self, predicate: impl Fn(GlobalPos) -> bool)
    {
        if self.unsaved_chunks.is_empty()
        {
            return;
        }

        while let Ok(finished_pos) = self.finish_rx.recv()
        {
            let count = self.unsaved_chunks.get_mut(&finished_pos).unwrap();
            *count -= 1;

            if *count == 0
            {
                self.unsaved_chunks.remove(&finished_pos);

                if predicate(finished_pos) || self.unsaved_chunks.is_empty()
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

    fn load_with<F>(&mut self, pos: GlobalPos, load_fn: F) -> Option<SaveT>
    where
        F: FnOnce(PathBuf, File) -> SaveT
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
}

impl<T> FileSave for FileSaver<T>
where
    for<'a> T: Saveable + Deserialize<'a> + Serialize
{
    type SaveItem = T;

    fn new(parent_path: PathBuf) -> Self
    {
        Self::new_with_saver(parent_path, |path, pair|
        {
            if let Err(err) = with_temp_save(Self::chunk_path(path, pair.key), &pair.value)
            {
                eprintln!("error saving {} to file: {err}", any::type_name::<T>());
            }
        })
    }

    fn save(&mut self, pair: ValuePair<Self::SaveItem>)
    {
        self.save_inner(pair);
    }

    fn load(&mut self, pos: GlobalPos) -> Option<Self::SaveItem>
    {
        self.load_with(pos, |_parent_path, file|
        {
            match load_compressed(file)
            {
                Ok(x) => x,
                Err(err) => panic!("error loading {} from file: {err}", any::type_name::<T>())
            }
        })
    }

    fn flush(&mut self)
    {
        self.flush();
    }
}

pub type SaveEntities = Vec<FullEntityInfo>;

pub type ChunkSaver = Saver<FileSaver<Chunk>, Chunk>;
pub type EntitiesSaver = Saver<FileSaver<SaveEntities>, SaveEntities>;
pub type WorldChunkSaver = Saver<FileSaver<WorldChunksBlock>, WorldChunksBlock>;

// again, shouldnt be public
#[derive(Debug)]
pub struct Saver<S, SaveT: Saveable>
where
    S: FileSave<SaveItem=SaveT>
{
    start: Instant,
    cache_amount: usize,
    cache: BinaryHeap<CachedValue<SaveT>>,
    file_saver: Arc<Mutex<S>>
}

impl<S, SaveT: Saveable> Saver<S, SaveT>
where
    S: FileSave<SaveItem=SaveT>
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
            self.cache.pop().unwrap();
        }
    }

    fn inner_load(&mut self, pos: GlobalPos) -> Option<SaveT>
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

    fn inner_save(&mut self, pair: ValuePair<SaveT>)
    {
        self.file_saver.lock().save(pair.clone());

        let key = CachedKey::new(self.start, pair.key);
        let value = CachedValue{key, value: pair.value};

        if self.cache.iter().any(|CachedValue{key, ..}| key.pos == pair.key)
        {
            self.cache.retain(|CachedValue{key, ..}| key.pos != pair.key);
        } else
        {
            self.free_cache(1);
        }

        self.cache.push(value);
    }
}

impl<T> SaveLoad<T> for Saver<FileSaver<T>, T>
where
    T: AutoSaveable + Clone,
    FileSaver<T>: FileSave<SaveItem=T>
{
    fn load(&mut self, pos: GlobalPos) -> Option<T>
    {
        self.inner_load(pos)
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

    use crate::{
        common::world::*,
        server::world::world_generator::*
    };

    use std::iter;


    fn clear_dir(dir_name: &Path)
    {
        if dir_name.exists()
        {
            fs::read_dir(dir_name).unwrap();
        }
    }

    fn dir_name(name: &str) -> PathBuf
    {
        PathBuf::from(name)
    }

    #[ignore]
    #[test]
    fn world_chunk_saving()
    {
        let dir_name = dir_name("test_world");

        clear_dir(&dir_name);

        // random ass numbers
        let size = Pos3::new(
            fastrand::usize(4..7),
            fastrand::usize(4..7),
            fastrand::usize(4..7)
        );

        let random_worldchunks = || -> [WorldChunk; CHUNK_RATIO.z]
        {
            (0..CHUNK_RATIO.z).map(|_|
            {
                WorldChunk::new(WorldChunkId::from_raw(fastrand::usize(0..100)), Vec::new())
            }).collect::<Vec<_>>().try_into().unwrap()
        };

        let mut chunks: Vec<_> = iter::repeat_with(||
            {
                random_worldchunks()
            }).zip((0..).map(|index|
            {
                GlobalPos::from(Pos3::from_rectangle(size, index))
            })).take(size.product() - 10 + fastrand::usize(0..20))
            .collect();

        let mut saver = WorldChunkSaver::new(&dir_name, 4);

        for (chunk, pos) in chunks.iter().cloned()
        {
            saver.save(pos, chunk);
        }

        for i in 1..10
        {
            saver.save(GlobalPos::from(size + i), random_worldchunks());
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

    #[ignore]
    #[test]
    fn world_chunk_reload()
    {
        let dir_name = dir_name("test_world_reload");

        clear_dir(&dir_name);

        let size = Pos3::new(
            fastrand::usize(4..7),
            fastrand::usize(4..7),
            fastrand::usize(4..7)
        );

        let random_worldchunks = || -> [WorldChunk; CHUNK_RATIO.z]
        {
            (0..CHUNK_RATIO.z).map(|_|
            {
                WorldChunk::new(WorldChunkId::from_raw(fastrand::usize(0..100)), Vec::new())
            }).collect::<Vec<_>>().try_into().unwrap()
        };

        let chunks: Vec<_> = iter::repeat_with(||
            {
                random_worldchunks()
            }).zip((0..).map(|index|
            {
                GlobalPos::from(Pos3::from_rectangle(size, index))
            })).take(size.product() - 10 + fastrand::usize(0..20))
            .collect();

        {
            let mut saver = WorldChunkSaver::new(&dir_name, 3);

            for (chunk, pos) in chunks.iter().cloned()
            {
                saver.save(pos, chunk.clone());
                assert_eq!(Some(chunk), saver.load(pos));
            }

            for (chunk, pos) in chunks.iter().cloned()
            {
                assert_eq!(Some(chunk), saver.load(pos));
            }
        }

        {
            let mut saver = WorldChunkSaver::new(&dir_name, 2);

            let compared: Vec<_> = chunks.iter().cloned().map(|(chunk, pos)|
            {
                Some(chunk) != saver.load(pos)
            }).collect();

            let total = compared.len();
            let wrongs: i32 = compared.into_iter().map(|x| if x { 1 } else { 0 }).sum();

            for (chunk, pos) in chunks.iter().cloned()
            {
                assert_eq!(Some(chunk), saver.load(pos), "{pos:?}, misses: {total}/{wrongs}");
            }

            clear_dir(&dir_name);

            dbg!("dropping saver");
            drop(saver);
            dbg!("dropping rest");
        }

        dbg!("dropped");
    }

    #[ignore]
    #[test]
    fn chunk_saving()
    {
        let dir_name = dir_name("test_world_normal");

        clear_dir(&dir_name);

        let size = Pos3::new(
            fastrand::usize(3..7),
            fastrand::usize(3..7),
            fastrand::usize(3..7)
        );

        let random_chunk = ||
        {
            Chunk::new_with(|_|
            {
                let mut tile = Tile::new(fastrand::usize(0..100));

                if let Some(tile) = tile.0.as_mut()
                {
                    let rotation = match fastrand::usize(0..4)
                    {
                        0 => TileRotation::Up,
                        1 => TileRotation::Right,
                        2 => TileRotation::Left,
                        3 => TileRotation::Down,
                        _ => unreachable!()
                    };

                    tile.set_rotation(rotation);
                }

                tile
            })
        };

        let chunks: Vec<_> = iter::repeat_with(||
            {
                random_chunk()
            }).zip((0..).map(|index|
            {
                GlobalPos::from(Pos3::from_rectangle(size, index))
            })).take(size.product() - 10 + fastrand::usize(0..20))
            .collect();

        {
            let mut saver = ChunkSaver::new(&dir_name, 4);

            for (chunk, pos) in chunks.iter().cloned()
            {
                saver.save(pos, chunk.clone());
                assert_eq!(Some(chunk), saver.load(pos));
            }

            for (chunk, pos) in chunks.iter().cloned()
            {
                assert_eq!(Some(chunk), saver.load(pos));
            }
        }

        let mut saver = ChunkSaver::new(&dir_name, 2);

        let compared: Vec<_> = chunks.iter().cloned().map(|(chunk, pos)|
        {
            Some(chunk) != saver.load(pos)
        }).collect();

        let total = compared.len();
        let wrongs: i32 = compared.into_iter().map(|x| if x { 1 } else { 0 }).sum();

        for (chunk, pos) in chunks.iter().cloned()
        {
            assert_eq!(Some(chunk), saver.load(pos), "{pos:?}, misses: {total}/{wrongs}");
        }

        clear_dir(&dir_name);
    }
}
