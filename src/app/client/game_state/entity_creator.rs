use crate::{
    client::UiElement,
    common::{
        render_info::*,
        LazyMix,
        Entity,
        ClientEntityInfo,
        entity::ClientEntities
    }
};


pub struct EntityCreator<'a>
{
    pub entities: &'a mut ClientEntities
}

impl EntityCreator<'_>
{
    pub fn push(
        &mut self,
        mut info: ClientEntityInfo,
        render: impl Into<Option<RenderInfo>>
    ) -> Entity
    {
        let default_ui = ||
        {
            UiElement{
                capture_events: false,
                ..Default::default()
            }
        };

        let render = render.into();
        if let Some(ref parent) = info.parent
        {
            let parent_entity = parent.entity();

            if self.entities.ui_element(parent_entity).map(|x| x.world_position).unwrap_or(false)
            {
                if info.ui_element.is_none()
                {
                    info.ui_element = Some(default_ui());
                }

                info.ui_element.as_mut().unwrap().world_position = true;
            }
        }

        if info.ui_element.is_none()
        {
            info.ui_element = Some(default_ui());
        }

        if info.watchers.is_none()
        {
            info.watchers = Some(Default::default());
        }

        if info.ui_element.is_some()
            && info.lazy_mix.is_none()
            && render.as_ref().map(|x| x.mix.is_none()).unwrap_or(false)
        {
            info.lazy_mix = Some(LazyMix::ui());
        }

        let entity = self.entities.push_client_eager(info);

        if let Some(mut render) = render
        {
            render.visibility_check = false;
            self.entities.set_deferred_render(entity, render);
        }

        entity
    }
}
