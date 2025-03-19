use std::{
    mem,
    collections::HashMap,
    hash::Hash
};

use nalgebra::Vector3;

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
        deferred: UiDeferredInfo,
        element: &UiElement
    ) -> Self
    {
        let transform = Transform{
            scale: Vector3::new(
                deferred.width.unwrap(),
                deferred.height.unwrap(),
                1.0
            ),
            ..Default::default()
        };

        let object = element.texture.name().and_then(|name|
        {
            RenderObject{
                kind: RenderObjectKind::Texture{name}
            }.into_client(transform, create_info)
        });

        Self{
            object
        }
    }
}

#[derive(Debug, Clone)]
pub struct UiDeferredInfo
{
    width: Option<f32>,
    height: Option<f32>
}

impl Default for UiDeferredInfo
{
    fn default() -> Self
    {
        Self{
            width: None,
            height: None
        }
    }
}

impl UiDeferredInfo
{
    fn screen() -> Self
    {
        Self{
            width: Some(1.0),
            height: Some(1.0)
        }
    }

    fn resolve(&mut self, element: &UiElement, parent: &Self)
    {
        if let Some(width) = parent.width
        {
            self.width = Some(element.width.resolve(SizeResolveInfo{
                parent: width
            }));
        }

        if let Some(height) = parent.height
        {
            self.height = Some(element.height.resolve(SizeResolveInfo{
                parent: height
            }));
        }
    }

    fn resolved(&self) -> bool
    {
        self.width.is_some() && self.height.is_some()
    }
}

#[derive(Debug)]
pub struct TreeElement<Id>
{
    element: UiElement,
    deferred: UiDeferredInfo,
    children: Vec<(Id, Self)>
}

impl<Id> TreeElement<Id>
{
    pub fn new(element: UiElement) -> Self
    {
        Self{
            element,
            deferred: UiDeferredInfo::default(),
            children: Vec::new()
        }
    }

    fn screen() -> Self
    {
        Self{
            element: UiElement{
                texture: UiTexture::None,
                ..Default::default()
            },
            deferred: UiDeferredInfo::screen(),
            children: Vec::new()
        }
    }

    pub fn update(&mut self, id: Id, element: UiElement) -> &mut Self
    {
        let index = self.children.len();
        self.children.push((id, Self::new(element)));

        &mut self.children[index].1
    }

    pub fn resolve_backward(&mut self)
    {
    }

    pub fn resolve_forward(&mut self, parent: &UiDeferredInfo)
    {
        if !self.deferred.resolved()
        {
            self.deferred.resolve(&self.element, parent);
        }

        self.children.iter_mut().for_each(|(_, x)| x.resolve_forward(&self.deferred));
    }

    pub fn resolved(&self) -> bool
    {
        self.deferred.resolved() && self.children.iter().all(|(_, x)| x.resolved())
    }

    fn for_each(self, id: Id, mut f: impl FnMut(Id, UiElement, UiDeferredInfo))
    {
        self.for_each_inner(id, &mut f)
    }

    fn for_each_inner(self, id: Id, f: &mut impl FnMut(Id, UiElement, UiDeferredInfo))
    {
        f(id, self.element, self.deferred);
        self.children.into_iter().for_each(|(id, child)| child.for_each_inner(id, f));
    }
}

#[derive(Debug)]
pub struct Controller<Id>
{
    order: Vec<Id>,
    created: Vec<(Id, UiElement, UiDeferredInfo)>,
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
        let empty = UiDeferredInfo::default();

        const LIMIT: usize = 1000;
        for i in 0..=LIMIT
        {
            self.root.resolve_backward();
            self.root.resolve_forward(&empty);

            if self.root.resolved()
            {
                break;
            }

            if i == LIMIT
            {
                panic!("couldnt resolve all deferred infos");
            }
        }

        mem::replace(&mut self.root, TreeElement::screen()).for_each(Id::screen(), |id, element, deferred|
        {
            self.order.push(id.clone());

            if let Some((old_element, _)) = self.elements.get(&id)
            {
                if *old_element == element
                {
                    return;
                }
            }

            self.created.push((id, element, deferred));
        });
    }

    pub fn create_renders(
        &mut self,
        create_info: &mut RenderCreateInfo
    )
    {
        self.prepare();

        mem::take(&mut self.created).into_iter().for_each(|(id, element, deferred)|
        {
            let element_cached = UiElementCached::from_element(create_info, deferred, &element);
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
