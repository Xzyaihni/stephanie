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

use crate::{
    server::world::world_generator::{CHUNK_RATIO, MaybeWorldChunk, WorldChunk},
    common::world::{
        Chunk,
        GlobalPos,
        Pos3
    }
};


// goes from 0 to 9, 0 being lowest level of compression
const LZMA_PRESET: u32 = 1;
const SAVE_MODULO: u32 = 20;

pub trait Saveable: Send + 'static {}

impl Saveable for Chunk {}
impl Saveable for SaveValueGroup {}

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
    file: File
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

        MaybeWorldChunk::from_bytes(&bytes).into()
    }
}

// again, shouldnt be public
#[derive(Debug)]
pub struct ValuePair<T>
{
    pub pos: GlobalPos,
    pub value: T
}

impl<T> ValuePair<T>
{
    pub fn new(pos: GlobalPos, value: T) -> Self
    {
        Self{pos, value}
    }
}

#[derive(Debug)]
struct InnerValue<S, T: Saveable>
where
    S: FileSave<SaveItem=T>
{
    file_saver: Arc<Mutex<S>>,
    pub pair: Option<ValuePair<T>>
}

impl<S, T: Saveable> Drop for InnerValue<S, T>
where
    S: FileSave<SaveItem=T>
{
    fn drop(&mut self)
    {
        self.file_saver.lock().save(self.pair.take().unwrap());
    }
}

impl<S, T: Saveable> InnerValue<S, T>
where
    S: FileSave<SaveItem=T>
{
    pub fn new(file_saver: Arc<Mutex<S>>, pair: ValuePair<T>) -> Self
    {
        Self{file_saver, pair: Some(pair)}
    }
}

#[derive(Debug)]
struct CachedValue<S, T: Saveable>
where
    S: FileSave<SaveItem=T>
{
    age: Duration,
    pub value: InnerValue<S, T>
}

impl<S, T: Saveable> CachedValue<S, T>
where
    S: FileSave<SaveItem=T>
{
    pub fn new(parent_path: Arc<Mutex<S>>, start: Instant, pair: ValuePair<T>) -> Self
    {
        Self{
            age: start.elapsed(),
            value: InnerValue::new(parent_path, pair)
        }
    }

    pub fn pos(&self) -> &GlobalPos
    {
        &self.value.pair.as_ref().unwrap().pos
    }

    pub fn value(&self) -> &T
    {
        &self.value.pair.as_ref().unwrap().value
    }
}

impl<S, T: Saveable> PartialEq for CachedValue<S, T>
where
    S: FileSave<SaveItem=T>
{
    fn eq(&self, other: &Self) -> bool
    {
        self.age.eq(&other.age)
    }
}

impl<S, T: Saveable> Eq for CachedValue<S, T>
where
    S: FileSave<SaveItem=T>
{
}

impl<S, T: Saveable> PartialOrd for CachedValue<S, T>
where
    S: FileSave<SaveItem=T>
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering>
    {
        Some(self.cmp(other))
    }
}

impl<S, T: Saveable> Ord for CachedValue<S, T>
where
    S: FileSave<SaveItem=T>
{
    fn cmp(&self, other: &Self) -> Ordering
    {
        other.age.cmp(&self.age)
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

    pub fn chunk_path(&self, pos: GlobalPos) -> PathBuf
    {
        Self::chunk_path_assoc(&self.parent_path, pos)
    }

    fn full_parent_path(&self, pos: GlobalPos) -> PathBuf
    {
        Self::full_parent_path_assoc(&self.parent_path, pos)
    }

    fn full_parent_path_assoc(parent_path: &Path, pos: GlobalPos) -> PathBuf
    {
        let pos_modulo = pos.0.map(|value| value / SAVE_MODULO as i32);

        parent_path
            .join(pos_modulo.z.to_string())
            .join(pos_modulo.y.to_string())
            .join(pos_modulo.x.to_string())
    }

    pub fn chunk_path_assoc(parent_path: &Path, pos: GlobalPos) -> PathBuf
    {
        Self::full_parent_path_assoc(parent_path, pos).join(Self::encode_position(pos))
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
        F: FnMut(PathBuf, T)
    {
        while let Ok(pair) = self.save_rx.recv()
        {
            fs::create_dir_all(self.full_parent_path(pair.pos)).unwrap();

            save_fn(self.chunk_path(pair.pos), pair.value);

            self.finish_tx.send(pair.pos).unwrap();
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
        F: FnMut(PathBuf, SaveT) + Send + 'static
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
        F: FnOnce(File) -> LoadT
    {
        if self.is_unsaved(pos)
        {
            self.block_until(pos);
        }

		match File::open(self.chunk_path(pos))
		{
			Ok(file) =>
			{
                Some(load_fn(file))
			},
			Err(ref err) if err.kind() == io::ErrorKind::NotFound =>
			{
				None
			},
			Err(err) => panic!("error loading chunk from file: {err:?}")
		}
    }

    fn save_inner(&mut self, pair: ValuePair<SaveT>)
    {
        let entry = self.unsaved_chunks.entry(pair.pos).or_insert(0);
        *entry += 1;

        self.save_tx.send(pair).unwrap();
    }

    fn chunk_path(&self, pos: GlobalPos) -> PathBuf
    {
        BlockingSaver::<SaveT>::chunk_path_assoc(&self.parent_path, pos)
    }
}

impl FileSave for FileSaver<Chunk>
{
    type SaveItem = Chunk;
    type LoadItem = Chunk;

	fn new(parent_path: PathBuf) -> Self
	{
        Self::new_with_saver(parent_path, |path, value|
        {
            let file = File::create(path).unwrap();

            let mut lzma_writer = LzmaWriter::new_compressor(file, LZMA_PRESET).unwrap();

            bincode::serialize_into(&mut lzma_writer, &value).unwrap();

            lzma_writer.finish().unwrap();
        })
    }

    fn save(&mut self, pair: ValuePair<Self::SaveItem>)
    {
        self.save_inner(pair);
    }

    fn load(&mut self, pos: GlobalPos) -> Option<Self::LoadItem>
    {
        self.load_with(pos, |file|
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
        Self::new_with_saver(parent_path, |path, value|
        {
            let file = match OpenOptions::new().write(true).open(&path)
            {
                Ok(file) => file,
                Err(ref err) if err.kind() == io::ErrorKind::NotFound =>
                {
                    let mut file = File::create(path).unwrap();

                    for _ in 0..CHUNK_RATIO.product()
                    {
                        MaybeWorldChunk::default().write_into(&mut file);
                    }

                    file
                },
                Err(err) => panic!("error loading worldchunk from file: {err:?}")
            };

            value.write_into(file);
        })
    }

    fn save(&mut self, pair: ValuePair<Self::SaveItem>)
    {
        self.save_inner(pair);
    }

    fn load(&mut self, pos: GlobalPos) -> Option<Self::LoadItem>
    {
        self.load_with(pos, |file|
        {
            LoadValueGroup{file}
        })
    }
}

pub type ChunkSaver = Saver<FileSaver<Chunk>, Chunk>;
pub type WorldChunkSaver = Saver<FileSaver<SaveValueGroup, LoadValueGroup>, SaveValueGroup, LoadValueGroup>;

// again, shouldnt be public
#[derive(Debug)]
pub struct Saver<S, SaveT: Saveable, LoadT=SaveT>
where
    S: FileSave<SaveItem=SaveT, LoadItem=LoadT>
{
    start: Instant,
    cache_amount: usize,
    cache: BinaryHeap<CachedValue<S, SaveT>>,
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
        let until_len = self.cache_amount - amount;

        while self.cache.len() > until_len
        {
            self.cache.pop().unwrap();
        }
    }

    fn inner_save(&mut self, pair: ValuePair<SaveT>)
    {
        self.free_cache(1);

        let value = CachedValue::new(self.file_saver.clone(), self.start, pair);

        self.cache.push(value);
    }
}

impl SaveLoad<WorldChunk> for WorldChunkSaver
{
	fn load(&mut self, pos: GlobalPos) -> Option<WorldChunk>
	{
        let index = WorldChunk::global_to_index(pos);

        let rounded_pos = WorldChunk::belongs_to(pos);

        if let Some(found) = self.cache.iter().find(|pair|
        {
            (*pair.pos() == rounded_pos) && (pair.value().index == index)
        })
        {
            return Some(found.value().value.clone());
        }

        self.file_saver.lock().load(rounded_pos)
            .and_then(|mut load_chunk| load_chunk.get(index))
	}

	fn save(&mut self, pos: GlobalPos, chunk: WorldChunk)
	{
        let index = WorldChunk::global_to_index(pos);

        let value = SaveValueGroup{value: chunk, index};
        let pair = ValuePair::new(WorldChunk::belongs_to(pos), value);

        self.inner_save(pair);
	}
}

impl SaveLoad<Chunk> for ChunkSaver
{
	fn load(&mut self, pos: GlobalPos) -> Option<Chunk>
	{
        if let Some(found) = self.cache.iter().find(|pair|
        {
            *pair.pos() == pos
        })
        {
            return Some(found.value().clone());
        }

        self.file_saver.lock().load(pos)
	}

	fn save(&mut self, pos: GlobalPos, chunk: Chunk)
	{
        let pair = ValuePair::new(pos, chunk);

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
                fs::read_dir(dir_name).unwrap()
                    .try_for_each(|entry|
                    {
                        let entry = entry?;

                        if !entry.file_type()?.is_file()
                        {
                            panic!("world directory should contain only files");
                        }

                        fs::remove_file(entry.path())
                    }).unwrap();

                fs::remove_dir(dir_name).unwrap();
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
