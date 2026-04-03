use std::{
    fs,
    borrow::Cow,
    path::PathBuf
};

use crate::{
    server::world::ParseError,
    common::{
        some_or_return,
        with_error,
        lisp::{self, *},
        Item,
        ItemsInfo,
        EnemyId,
        FurnitureId,
        world::Tile
    }
};


pub fn loot_compile(code: &str) -> Generator
{
    with_error(Generator::new(code)).unwrap_or_default()
}

#[derive(Clone)]
pub struct Generator(Option<Lisp>);

impl Default for Generator
{
    fn default() -> Self
    {
        Self::new_empty()
    }
}

impl Generator
{
    pub fn new(code: &str) -> Result<Self, ParseError>
    {
        let load = |filename|
        {
            fs::read_to_string(filename)
                .map_err(|err| ParseError::new_named(PathBuf::from(filename), err))
        };

        let standard = load("lisp/standard.scm")?;
        let loot_standard = load("lisp/loot.scm")?;

        Ok(Self(Some(Lisp::new(&[&standard, &loot_standard, code]).map_err(ParseError::new)?)))
    }

    pub fn new_empty() -> Self
    {
        Self(None)
    }

    pub fn create<'a>(&self, items_info: &ItemsInfo, name: impl Fn() -> Cow<'a, str>) -> Vec<Item>
    {
        let lisp = some_or_return!(self.0.as_ref());

        let (memory, value) = match lisp.run()
        {
            Ok(x) => x.destructure(),
            Err(err) =>
            {
                let name = name();
                let source = ["standard", "loot", &name][err.position.source];
                eprintln!("(in {source}) {err}");

                return Vec::new();
            }
        };

        let items = match value.as_pairs_list(&memory)
        {
            Ok(x) => x,
            Err(err) =>
            {
                eprintln!("{}: {err}", &name());

                return Vec::new();
            }
        };

        items.into_iter().filter_map(move |item|
        {
            let item = Self::parse_item(items_info, OutputWrapperRef::new(&memory, item));

            match item
            {
                Ok(x) => Some(x),
                Err(err) =>
                {
                    eprintln!("{}: {err}", &name());

                    None
                }
            }
        }).collect()
    }

    fn parse_item(items_info: &ItemsInfo, value: OutputWrapperRef) -> Result<Item, lisp::Error>
    {
        let name = value.as_symbol().map(|name|
        {
            name.chars().map(|c| if c == '_' { ' ' } else { c }).collect::<String>()
        }).or_else(|_|
        {
            value.as_string()
        })?;

        let id = items_info.get_id(&name).ok_or_else(||
        {
            lisp::Error::Custom(format!("item named {name} not found"))
        })?;

        Ok(Item::new(items_info, id))
    }
}

#[derive(Clone)]
pub struct ServerFurnitureLootInfo<T>
{
    pub on_contents: T
}

impl<T> ServerFurnitureLootInfo<T>
{
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> ServerFurnitureLootInfo<U>
    {
        ServerFurnitureLootInfo{
            on_contents: f(self.on_contents)
        }
    }
}

#[derive(Clone)]
pub struct ClientFurnitureLootInfo
{
    pub on_destroy: Generator
}

#[derive(Clone)]
pub struct EnemyLootInfo<T>
{
    pub on_contents: T,
    pub on_equip: T
}

impl<T> EnemyLootInfo<T>
{
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> EnemyLootInfo<U>
    {
        EnemyLootInfo{
            on_contents: f(self.on_contents),
            on_equip: f(self.on_equip)
        }
    }
}

#[derive(Clone, Default)]
pub struct TileLootInfo
{
    pub on_destroy: Generator
}

#[derive(Clone)]
pub struct ServerLootInfo
{
    pub furniture: Vec<ServerFurnitureLootInfo<Option<String>>>,
    pub enemy: Vec<EnemyLootInfo<Option<String>>>
}

#[derive(Clone, Default)]
pub struct ServerLoot
{
    pub furniture: Vec<ServerFurnitureLootInfo<Generator>>,
    pub enemy: Vec<EnemyLootInfo<Generator>>
}

impl ServerLoot
{
    pub fn furniture_generator(&self, id: FurnitureId) -> &ServerFurnitureLootInfo<Generator>
    {
        &self.furniture[usize::from(id)]
    }

    pub fn enemy_generator(&self, id: EnemyId) -> &EnemyLootInfo<Generator>
    {
        &self.enemy[usize::from(id)]
    }
}

#[derive(Clone)]
pub struct ClientLoot
{
    pub furniture: Vec<ClientFurnitureLootInfo>,
    pub tile: Vec<TileLootInfo>,
    pub empty: TileLootInfo
}

impl ClientLoot
{
    pub fn furniture_generator(&self, id: FurnitureId) -> &ClientFurnitureLootInfo
    {
        &self.furniture[usize::from(id)]
    }

    pub fn tile_generator(&self, id: Tile) -> &TileLootInfo
    {
        id.id().map(|id| &self.tile[id]).unwrap_or(&self.empty)
    }
}
