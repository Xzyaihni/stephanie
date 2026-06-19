use std::{
    fs,
    cell::RefCell
};

use crate::{
    server::world::ParseError,
    common::{
        some_or_return,
        with_error,
        lisp::{self, *},
        Door,
        Item,
        ItemsInfo,
        EnemyId,
        FurnitureId,
        world::Tile
    }
};


pub fn loot_compile(name: String, code: &str) -> Generator
{
    with_error(Generator::new(name, code)).unwrap_or_default()
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

fn create_with_lisp<'a>(items_info: &ItemsInfo, lisp: &Lisp) -> Vec<Item>
{
    let (memory, value) = match lisp.run()
    {
        Ok(x) => x.destructure(),
        Err(err) =>
        {
            let source = lisp.get_source(err.position.source);
            eprintln!("(in {source}) {err}");

            return Vec::new();
        }
    };

    let items = match value.as_pairs_list(&memory)
    {
        Ok(x) => x,
        Err(err) =>
        {
            eprintln!("{}: {err}", lisp.get_source(0));

            return Vec::new();
        }
    };

    items.into_iter().filter_map(move |item|
    {
        let item = parse_item(items_info, OutputWrapperRef::new(&memory, item));

        match item
        {
            Ok(x) => Some(x),
            Err(err) =>
            {
                eprintln!("{}: {err}", lisp.get_source(0));

                None
            }
        }
    }).collect()
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
    pub fn new(name: String, code: &str) -> Result<Self, ParseError>
    {
        let mut lisp = Lisp::new_one(code).map_err(ParseError::new)?;
        lisp.set_source_name(0, name);

        Ok(Self(Some(lisp)))
    }

    pub fn new_empty() -> Self
    {
        Self(None)
    }

    pub fn create(&self, items_info: &ItemsInfo) -> Vec<Item>
    {
        let lisp = some_or_return!(self.0.as_ref());

        create_with_lisp(items_info, lisp)
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
    pub on_create: T,
    pub on_contents: T,
    pub on_equip: T
}

impl<T> EnemyLootInfo<T>
{
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> EnemyLootInfo<U>
    {
        EnemyLootInfo{
            on_create: f(self.on_create),
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
pub struct ServerLootSingleInfo
{
    pub name: String,
    pub code: String
}

#[derive(Clone)]
pub struct ServerLootInfo
{
    pub furniture: Vec<ServerFurnitureLootInfo<Option<ServerLootSingleInfo>>>,
    pub enemy: Vec<EnemyLootInfo<Option<ServerLootSingleInfo>>>
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
pub struct DoorGenerator
{
    lisp: RefCell<Lisp>
}

impl Default for DoorGenerator
{
    fn default() -> Self
    {
        Self{lisp: RefCell::new(Lisp::default())}
    }
}

impl DoorGenerator
{
    pub fn new(path: &str) -> Self
    {
        let code = some_or_return!(with_error(fs::read_to_string(path)));

        with_error(Lisp::new_one(&code)).map(|mut lisp|
        {
            lisp.set_source_name(0, "door".to_owned());

            Self{lisp: RefCell::new(lisp)}
        }).unwrap_or_default()
    }

    pub fn create(&self, items_info: &ItemsInfo, door: &Door) -> Vec<Item>
    {
        let mut lisp = self.lisp.borrow_mut();

        let material_name = <&str>::from(door.material());

        {
            let memory = lisp.memory_mut();

            let material_symbol = memory.new_symbol(material_name);

            some_or_return!(with_error(memory.define("material", material_symbol)));
            some_or_return!(with_error(memory.define("width", (door.width() as i32).into())));
        }

        create_with_lisp(items_info, &lisp)
    }
}

#[derive(Clone)]
pub struct ClientLoot
{
    pub furniture: Vec<ClientFurnitureLootInfo>,
    pub tile: Vec<TileLootInfo>,
    pub empty: TileLootInfo,
    pub door: DoorGenerator
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
