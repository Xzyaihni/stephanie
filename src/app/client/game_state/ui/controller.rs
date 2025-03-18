use std::{
    mem,
    collections::HashMap,
    hash::Hash
};

use yanyaengine::{
    Transform,
    game_object::*
};

use crate::{
    client::RenderCreateInfo,
    common::render_info::*
};

use super::element::*;


pub trait UiIdable
{
    fn screen() -> Self;
}

#[derive(Debug)]
struct UiElementCached
{
    object: Option<ClientRenderObject>
}

impl UiElementCached
{
    fn from_element(
        create_info: &mut RenderCreateInfo,
        element: &UiElement
    ) -> Self
    {
        let transform = Transform::default();
        let object = RenderObject{
            kind: RenderObjectKind::Texture{name: element.texture.name()}
        }.into_client(transform, create_info);

        Self{
            object
        }
    }
}

#[derive(Debug)]
pub struct TreeElement<Id>
{
    element: UiElement,
    children: Vec<(Id, Self)>
}

impl<Id> TreeElement<Id>
{
    pub fn new(element: UiElement) -> Self
    {
        Self{
            element,
            children: Vec::new()
        }
    }

    fn screen() -> Self
    {
        Self::new(UiElement{
            ..Default::default()
        })
    }

    pub fn update(&mut self, id: Id, element: UiElement) -> &mut Self
    {
        let index = self.children.len();
        self.children.push((id, Self::new(element)));

        &mut self.children[index].1
    }

    fn for_each(self, id: Id, mut f: impl FnMut(Id, UiElement))
    {
        self.for_each_inner(id, &mut f)
    }

    fn for_each_inner(self, id: Id, f: &mut impl FnMut(Id, UiElement))
    {
        f(id, self.element);
        self.children.into_iter().for_each(|(id, child)| child.for_each_inner(id, f));
    }
}

#[derive(Debug)]
pub struct Controller<Id>
{
    order: Vec<Id>,
    created: Vec<(Id, UiElement)>,
    elements: HashMap<Id, (UiElement, UiElementCached)>,
    root: TreeElement<Id>
}

impl<Id: Hash + Eq + Clone + UiIdable> Controller<Id>
{
    pub fn new() -> Self
    {
        Self{
            order: Vec::new(),
            created: Vec::new(),
            elements: HashMap::new(),
            root: TreeElement::screen()
        }
    }

    pub fn begin(&mut self)
    {
        self.order.clear();
    }

    pub fn update(&mut self, id: Id, element: UiElement) -> &mut TreeElement<Id>
    {
        debug_assert!(!self.order.contains(&id));

        self.root.update(id, element)
    }

    fn prepare(&mut self)
    {
        mem::replace(&mut self.root, TreeElement::screen()).for_each(Id::screen(), |id, element|
        {
            self.order.push(id.clone());

            if let Some((old_element, _)) = self.elements.get(&id)
            {
                if *old_element == element
                {
                    return;
                }
            }

            self.created.push((id, element));
        });
    }

    pub fn create_renders(
        &mut self,
        create_info: &mut RenderCreateInfo
    )
    {
        self.prepare();

        mem::take(&mut self.created).into_iter().for_each(|(id, element)|
        {
            let element_cached = UiElementCached::from_element(create_info, &element);
            if let Some((old_element, old_element_cached)) = self.elements.get_mut(&id)
            {
                // do fancy ops here later
                *old_element = element;
                *old_element_cached = element_cached;
            } else
            {
                self.elements.insert(id, (element, element_cached));
            }
        });
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        self.elements.values_mut().for_each(|(_, element)|
        {
            if let Some(object) = element.object.as_mut()
            {
                object.update_buffers(info)
            }
        });
    }

    pub fn draw(
        &self,
        info: &mut DrawInfo
    )
    {
        self.order.iter().for_each(|id|
        {
            let (element, element_cached) = &self.elements[id];
            if let Some(object) = element_cached.object.as_ref()
            {
                info.push_constants(UiOutlinedInfo::new(element.mix));

                object.draw(info)
            }
        });
    }
}
