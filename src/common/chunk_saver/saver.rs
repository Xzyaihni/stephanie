use std::{
    io,
    path::PathBuf,
    fs::{self, File},
    fmt::{self, Display}
};

use lzma::{LzmaWriter, LzmaReader};

use serde::{Serialize, de::DeserializeOwned};


// goes from 0 to 9, 0 being lowest level of compression
const LZMA_PRESET: u32 = 1;

pub enum SaveError
{
    Io(io::Error),
    Lzma(lzma::LzmaError),
    Json(serde_json::Error),
    Bincode(bincode::error::EncodeError)
}

impl From<io::Error> for SaveError
{
    fn from(value: io::Error) -> Self
    {
        Self::Io(value)
    }
}

impl From<lzma::LzmaError> for SaveError
{
    fn from(value: lzma::LzmaError) -> Self
    {
        Self::Lzma(value)
    }
}

impl From<serde_json::Error> for SaveError
{
    fn from(value: serde_json::Error) -> Self
    {
        Self::Json(value)
    }
}

impl From<bincode::error::EncodeError> for SaveError
{
    fn from(value: bincode::error::EncodeError) -> Self
    {
        Self::Bincode(value)
    }
}

impl Display for SaveError
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::Io(x) => Display::fmt(x, f),
            Self::Lzma(x) => Display::fmt(x, f),
            Self::Json(x) => Display::fmt(x, f),
            Self::Bincode(x) => Display::fmt(x, f)
        }
    }
}

pub fn json_loader<T: DeserializeOwned>() -> fn(File) -> serde_json::Result<T>
{
    |file|
    {
        serde_json::from_reader(file)
    }
}

pub fn compressed_loader<T: DeserializeOwned>() -> fn(File) -> Result<T, LoadError>
{
    |file|
    {
        load_compressed(file)
    }
}

pub fn json_saver<T: Serialize>(value: &T) -> impl FnOnce(File) -> Result<(), SaveError> + use<'_, T>
{
    move |file|
    {
        serde_json::to_writer(file, value)?;

        Ok(())
    }
}

pub fn compressed_saver<T: Serialize>(value: T) -> impl FnOnce(File) -> Result<(), SaveError>
{
    move |file|
    {
        let mut lzma_writer = LzmaWriter::new_compressor(file, LZMA_PRESET)?;

        bincode::serde::encode_into_std_write(value, &mut lzma_writer, crate::common::BINCODE_CONFIG)?;

        lzma_writer.finish()?;

        Ok(())
    }
}

pub fn with_temp_save(path: PathBuf, saver: impl FnOnce(File) -> Result<(), SaveError>) -> Result<(), SaveError>
{
    let temp_path = path.with_extension("tmp");

    let file = File::create(&temp_path)?;

    saver(file)?;

    fs::rename(temp_path, path)?;

    Ok(())
}

pub enum LoadError
{
    Lzma(lzma::LzmaError),
    Bincode(bincode::error::DecodeError)
}

impl From<lzma::LzmaError> for LoadError
{
    fn from(value: lzma::LzmaError) -> Self
    {
        Self::Lzma(value)
    }
}

impl From<bincode::error::DecodeError> for LoadError
{
    fn from(value: bincode::error::DecodeError) -> Self
    {
        Self::Bincode(value)
    }
}

impl Display for LoadError
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::Lzma(x) => Display::fmt(x, f),
            Self::Bincode(x) => Display::fmt(x, f)
        }
    }
}

pub fn load_compressed<T: DeserializeOwned>(file: File) -> Result<T, LoadError>
{
    let mut lzma_reader = LzmaReader::new_decompressor(file)?;

    Ok(bincode::serde::decode_from_std_read(&mut lzma_reader, crate::common::BINCODE_CONFIG)?)
}
