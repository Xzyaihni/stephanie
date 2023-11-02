use std::{
	io,
    thread,
    marker::PhantomData,
    cmp::Ordering,
    time::{Instant, Duration},
    path::PathBuf,
	fs::{self, File},
    collections::{HashMap, BinaryHeap},
    sync::{
        Arc,
        mpsc::{self, Sender, Receiver}
    }
};

use parking_lot::Mutex;

use lzma::{LzmaWriter, LzmaReader};

use serde::{Serialize, de::DeserializeOwned};

use crate::common::world::{GlobalPos, Pos3};


// goes from 0 to 9, 0 being lowest level of compression
const LZMA_PRESET: u32 = 1;

pub trait Saveable: Serialize + DeserializeOwned + Send + 'static {}

pub trait Saver<T>
{
	fn save(&mut self, pos: GlobalPos, chunk: T);
	fn load(&mut self, pos: GlobalPos) -> Option<T>;
}

#[derive(Debug)]
struct ValuePair<T>
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
struct InnerValue<T: Saveable>
{
    file_saver: Arc<Mutex<FileSaver<T>>>,
    pub pair: Option<ValuePair<T>>
}

impl<T: Saveable> Drop for InnerValue<T>
{
    fn drop(&mut self)
    {
        self.file_saver.lock().save(self.pair.take().unwrap());
    }
}

impl<T: Saveable> InnerValue<T>
{
    pub fn new(file_saver: Arc<Mutex<FileSaver<T>>>, pair: ValuePair<T>) -> Self
    {
        Self{file_saver, pair: Some(pair)}
    }
}

#[derive(Debug)]
struct CachedValue<T: Saveable>
{
    age: Duration,
    pub value: InnerValue<T>
}

impl<T: Saveable> CachedValue<T>
{
    pub fn new(parent_path: Arc<Mutex<FileSaver<T>>>, start: Instant, pair: ValuePair<T>) -> Self
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

impl<T: Saveable> PartialEq for CachedValue<T>
{
    fn eq(&self, other: &Self) -> bool
    {
        self.age.eq(&other.age)
    }
}

impl<T: Saveable> Eq for CachedValue<T> {}

impl<T: Saveable> PartialOrd for CachedValue<T>
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering>
    {
        Some(self.cmp(other))
    }
}

impl<T: Saveable> Ord for CachedValue<T>
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

    fn chunk_path(&self, pos: GlobalPos) -> PathBuf
    {
        Self::chunk_path_assoc(&self.parent_path, pos)
    }

    pub fn chunk_path_assoc(parent_path: &PathBuf, pos: GlobalPos) -> PathBuf
    {
        parent_path.join(Self::encode_position(pos))
    }

    fn encode_position(pos: GlobalPos) -> String
    {
        let GlobalPos(Pos3{x, y, z}) = pos;

        format!("{x}_{y}_{z}")
    }
}

impl<T: Serialize> BlockingSaver<T>
{
    pub fn run(self)
    {
        while let Ok(pair) = self.save_rx.recv()
        {
            let file = File::create(self.chunk_path(pair.pos)).unwrap();

            let mut lzma_writer = LzmaWriter::new_compressor(file, LZMA_PRESET).unwrap();

            bincode::serialize_into(&mut lzma_writer, &pair.value).unwrap();

            lzma_writer.finish().unwrap();

            self.finish_tx.send(pair.pos).unwrap();
        }
    }
}

#[derive(Debug)]
struct FileSaver<T: Serialize>
{
    parent_path: PathBuf,
    // i need the usize field just to count the saves called for the same chunk
    unsaved_chunks: HashMap<GlobalPos, usize>,
    save_tx: Sender<ValuePair<T>>,
    finish_rx: Receiver<GlobalPos>,
    phantom: PhantomData<T>
}

impl<T: Saveable> FileSaver<T>
{
	pub fn new(parent_path: PathBuf) -> Self
	{
        let (save_tx, save_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();

        {
            let parent_path = parent_path.clone();

            thread::spawn(||
            {
                let saver: BlockingSaver<T> = BlockingSaver::new(parent_path, save_rx, finish_tx);

                saver.run();
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

    pub fn load(&mut self, pos: GlobalPos) -> Option<T>
    {
        if self.is_unsaved(pos)
        {
            self.block_until(pos);
        }

		match File::open(self.chunk_path(pos))
		{
			Ok(file) =>
			{
                let lzma_reader = LzmaReader::new_decompressor(file).unwrap();

				Some(bincode::deserialize_from(lzma_reader).unwrap())
			},
			Err(ref err) if err.kind() == io::ErrorKind::NotFound =>
			{
				None
			},
			Err(err) => panic!("error loading chunk from file: {:?}", err)
		}
    }

    pub fn save(&mut self, pair: ValuePair<T>)
    {
        let entry = self.unsaved_chunks.entry(pair.pos).or_insert(0);
        *entry += 1;

        self.save_tx.send(pair).unwrap();
    }

    fn chunk_path(&self, pos: GlobalPos) -> PathBuf
    {
        BlockingSaver::<T>::chunk_path_assoc(&self.parent_path, pos)
    }
}

#[derive(Debug)]
pub struct ChunkSaver<T: Saveable>
{
    start: Instant,
    cache_amount: usize,
    cache: BinaryHeap<CachedValue<T>>,
    file_saver: Arc<Mutex<FileSaver<T>>>
}

impl<T: Saveable> ChunkSaver<T>
{
	pub fn new(parent_path: impl Into<PathBuf>, cache_amount: usize) -> Self
	{
        let parent_path = parent_path.into();

		fs::create_dir_all(&parent_path).unwrap();

        let file_saver = FileSaver::new(parent_path.into());

		Self{
            start: Instant::now(),
            file_saver: Arc::new(Mutex::new(file_saver)),
            cache_amount,
            cache: BinaryHeap::new()
        }
	}
}

impl<T: Saveable + Clone> Saver<T> for ChunkSaver<T>
{
	fn load(&mut self, pos: GlobalPos) -> Option<T>
	{
        if let Some(found) = self.cache.iter().find(|chunk|
        {
            *chunk.pos() == pos
        })
        {
            return Some(found.value().clone());
        }

        self.file_saver.lock().load(pos)
	}

	fn save(&mut self, pos: GlobalPos, chunk: T)
	{
        while self.cache.len() >= self.cache_amount
        {
            self.cache.pop().unwrap();
        }

        let pair = ValuePair::new(pos, chunk);
        let value = CachedValue::new(self.file_saver.clone(), self.start, pair);

        self.cache.push(value);
	}
}
