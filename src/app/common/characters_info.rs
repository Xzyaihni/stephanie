use yanyaengine::TextureId;

use crate::common::generic_info::define_info_id;


define_info_id!{CharacterId}

pub struct CharacterInfo
{
    pub scale: f32,
    pub normal: TextureId,
    pub lying: TextureId
}

#[derive(Default)]
pub struct CharactersInfo
{
    items: Vec<CharacterInfo>
}

impl CharactersInfo
{
    pub fn push(&mut self, item: CharacterInfo) -> CharacterId
    {
        let id = self.items.len();

        self.items.push(item);

        CharacterId(id)
    }

    pub fn get(&self, id: CharacterId) -> &CharacterInfo
    {
        &self.items[id.0]
    }
}
