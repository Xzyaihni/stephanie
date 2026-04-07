use std::{
    fs,
    ops::{Deref, DerefMut},
    time::SystemTime,
    path::{Path, PathBuf}
};

use crate::client::ui_common::TextboxInfo;


#[derive(Debug, Clone, PartialEq)]
pub struct ModifiedWatcher<T>
{
    paths: Vec<PathBuf>,
    last_modified: Vec<Option<SystemTime>>,
    value: T
}

impl<T> Deref for ModifiedWatcher<T>
{
    type Target = T;

    fn deref(&self) -> &Self::Target
    {
        &self.value
    }
}

impl<T> DerefMut for ModifiedWatcher<T>
{
    fn deref_mut(&mut self) -> &mut Self::Target
    {
        &mut self.value
    }
}

pub fn modified_time(path: &Path) -> Option<SystemTime>
{
    if !path.exists()
    {
        eprintln!("cant find path: {}", path.display());
        return None;
    }

    let this_metadata = match fs::metadata(path)
    {
        Ok(x) => x,
        Err(err) =>
        {
            eprintln!("modified time access error: {err}");
            return None;
        }
    };

    if this_metadata.is_dir()
    {
        match fs::read_dir(path)
        {
            Ok(x) =>
            {
                x.fold(None, |acc, x|
                {
                    let modified_time = match x
                    {
                        Ok(x) => modified_time(&x.path()),
                        Err(err) =>
                        {
                            eprintln!("dir entry error: {err}");
                            None
                        }
                    };

                    if let Some(modified) = modified_time
                    {
                        if let Some(acc) = acc
                        {
                            Some(if modified > acc { modified } else { acc })
                        } else
                        {
                            Some(modified)
                        }
                    } else
                    {
                        acc
                    }
                })
            },
            Err(err) =>
            {
                eprintln!("read dir error: {err}");

                None
            }
        }
    } else
    {
        match this_metadata.modified()
        {
            Ok(x) => Some(x),
            Err(err) =>
            {
                eprintln!("modified access error: {err}");

                None
            }
        }
    }
}

impl<T> ModifiedWatcher<T>
{
    pub fn new(path: impl Into<PathBuf>, value: T) -> Self
    {
        let path = path.into();

        Self::new_many(vec![path], value)
    }

    pub fn new_many(paths: Vec<PathBuf>, value: T) -> Self
    {
        let last_modified: Vec<_> = paths.iter().map(|x| modified_time(x)).collect();

        Self{
            paths,
            last_modified,
            value
        }
    }

    pub fn modified_check(&mut self) -> bool
    {
        self.paths.iter().zip(self.last_modified.iter_mut()).fold(false, |modified, (path, last_modified)|
        {
            let new_modified_time = modified_time(path);

            let changed = new_modified_time != *last_modified;

            if changed
            {
                *last_modified = new_modified_time;
            }

            modified || changed
        })
    }
}

#[derive(Debug, Clone)]
pub struct TextboxWrapper(pub TextboxInfo);

impl From<String> for TextboxWrapper
{
    fn from(s: String) -> Self
    {
        Self(TextboxInfo::new(s))
    }
}

impl PartialEq for TextboxWrapper
{
    fn eq(&self, other: &Self) -> bool
    {
        self.0.text == other.0.text
    }
}
