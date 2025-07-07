use std::{fmt, f32};

use serde::{
    Serialize,
    Deserialize,
    Serializer,
    ser::SerializeTuple,
    Deserializer,
    de::{EnumAccess, VariantAccess, Visitor}
};

use strum::{FromRepr, EnumString};

use crate::common::{
    some_or_return,
    TileMap,
    DamageType,
    lisp::{self, *}
};


#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tile(pub Option<TileExisting>);

impl Serialize for Tile
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error>
    {
        if let Some(tile) = self.0
        {
            let id = tile.id * 2;
            if let Some(info) = tile.info.as_ref()
            {
                let mut t = serializer.serialize_tuple(2)?;

                t.serialize_element(&(id + 2))?;
                t.serialize_element(info)?;

                t.end()
            } else
            {
                serializer.serialize_u64(id as u64 + 1)
            }
        } else
        {
            serializer.serialize_u64(0)
        }
    }
}

struct TileVisitor;

impl<'de> Visitor<'de> for TileVisitor
{
    type Value = Tile;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "a tile")
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<Self::Value, A::Error>
    {
        let (id, variant): (usize, A::Variant) = data.variant()?;

        let tile = if id == 0
        {
            Tile(None)
        } else if id % 2 == 1
        {
            Tile(Some(TileExisting{id: id / 2, info: None}))
        } else
        {
            let info: TileInfo = variant.newtype_variant()?;

            Tile(Some(TileExisting{id: id / 2 - 1, info: Some(info)}))
        };

        Ok(tile)
    }
}

impl<'de> Deserialize<'de> for Tile
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>
    {
        deserializer.deserialize_enum("tile", &["none", "tileinfoless", "tileinfo"], TileVisitor)
    }
}

impl Tile
{
    pub fn new(id: usize) -> Self
    {
        Self(Some(TileExisting{id, info: None}))
    }

    pub fn visual_eq(&self, other: &Self) -> bool
    {
        if let (Some(a), Some(b)) = (&self.0, &other.0)
        {
            a.visual_eq(b)
        } else
        {
            false
        }
    }

    pub fn id_string(&self) -> String
    {
        if let Some(tile) = self.0
        {
            tile.id_string()
        } else
        {
            "_".to_owned()
        }
    }

    pub fn as_lisp_value(&self, memory: &mut LispMemory) -> Result<LispValue, lisp::Error>
    {
        self.0.map(|x| x.as_lisp_value(memory)).unwrap_or_else(|| Ok(().into()))
    }

    pub fn from_lisp_value(value: OutputWrapperRef) -> Result<Self, lisp::Error>
    {
        if value.is_null()
        {
            return Ok(Tile::none());
        }

        let lst = value.as_list()?;

        let id = lst.car.as_integer()? as usize;

        let info = TileInfo::from_lisp_value(lst.cdr)?;

        Ok(Tile(Some(TileExisting::new(id, info))))
    }

    pub fn id(&self) -> Option<usize>
    {
        self.0.map(|x| x.id)
    }

    pub fn none() -> Self
    {
        Self::default()
    }

    pub fn is_none(&self) -> bool
    {
        self.0.is_none()
    }

    pub fn damage(&mut self, tilemap: &TileMap, damage: DamageType) -> bool
    {
        let this_tile = *self;
        let tile = some_or_return!(&mut self.0);

        let mut info = tile.info.unwrap_or_default();
        let health_fraction = info.health_fraction;

        let tile_info = tilemap.info(this_tile);
        let health = health_fraction * tile_info.health;

        let damage = damage.as_flat() * 0.0001;

        let new_health = health - damage;

        let destroyed = new_health < 0.0;

        if destroyed
        {
            self.0 = None;
        } else
        {
            info.health_fraction = new_health / tile_info.health;
            tile.info = Some(info);
        }

        destroyed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, FromRepr, EnumString, Serialize, Deserialize)]
#[strum(ascii_case_insensitive)]
pub enum TileRotation
{
    Up,
    Right,
    Left,
    Down
}

impl Default for TileRotation
{
    fn default() -> Self
    {
        Self::Up
    }
}

impl TileRotation
{
    pub fn to_angle(&self) -> f32
    {
        match self
        {
            Self::Right => 0.0,
            Self::Up => f32::consts::FRAC_PI_2,
            Self::Left => f32::consts::PI,
            Self::Down => -f32::consts::FRAC_PI_2
        }
    }

    pub fn to_arrow_str(&self) -> &str
    {
        match self
        {
            Self::Up => "↑",
            Self::Right => "→",
            Self::Left => "←",
            Self::Down => "↓"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileExisting
{
    id: usize,
    info: Option<TileInfo>
}

impl TileExisting
{
    pub fn new(id: usize, info: Option<TileInfo>) -> Self
    {
        let mut this = Self{id, info};
        this.simplify();

        this
    }

    pub fn id(&self) -> usize
    {
        self.id
    }

    pub fn visual_eq(&self, other: &Self) -> bool
    {
        self.id == other.id
            && self.info.unwrap_or_default().visual_eq(&other.info.unwrap_or_default())
    }

    pub fn id_string(&self) -> String
    {
        format!("{}{}", self.id, self.info.unwrap_or_default().id_string())
    }

    pub fn rotation(&self) -> TileRotation
    {
        self.info.map(|x| x.rotation).unwrap_or_default()
    }

    pub fn set_rotation(&mut self, rotation: TileRotation)
    {
        if let Some(info) = self.info.as_mut()
        {
            info.rotation = rotation;
        } else
        {
            self.info = Some(TileInfo{rotation, ..Default::default()});
        }

        self.simplify();
    }

    fn simplify(&mut self)
    {
        if let Some(info) = self.info
        {
            if info == TileInfo::default()
            {
                self.info = None;
            }
        }
    }

    pub fn as_lisp_value(&self, memory: &mut LispMemory) -> Result<LispValue, lisp::Error>
    {
        let restore = memory.with_saved_registers([Register::Value, Register::Temporary]);

        memory.set_register(Register::Value, self.id as i32);

        let value = self.info.unwrap_or_default().as_lisp_value(memory)?;
        memory.set_register(Register::Temporary, value);

        memory.cons(Register::Value, Register::Value, Register::Temporary)?;

        let value = memory.get_register(Register::Value);

        restore(memory)?;

        Ok(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TileInfo
{
    pub rotation: TileRotation,
    pub health_fraction: f32
}

impl Default for TileInfo
{
    fn default() -> Self
    {
        Self{rotation: TileRotation::default(), health_fraction: 1.0}
    }
}

impl TileInfo
{
    pub fn visual_eq(&self, other: &Self) -> bool
    {
        self.rotation == other.rotation
    }

    pub fn from_lisp_value(value: OutputWrapperRef) -> Result<Option<Self>, lisp::Error>
    {
        if value.is_null()
        {
            return Ok(None);
        }

        let rotation = value.as_integer()?;
        let rotation = TileRotation::from_repr(rotation as usize).unwrap_or_else(||
        {
            panic!("{rotation} is an invalid rotation number")
        });

        Ok(Some(TileInfo{rotation, ..Default::default()}))
    }

    pub fn as_lisp_value(&self, _memory: &mut LispMemory) -> Result<LispValue, lisp::Error>
    {
        Ok((self.rotation as i32).into())
    }

    pub fn id_string(&self) -> String
    {
        self.rotation.to_arrow_str().to_owned()
    }
}
