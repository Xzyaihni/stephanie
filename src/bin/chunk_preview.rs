use std::{
    fs,
    rc::Rc,
    path::PathBuf
};

use vulkano::pipeline::graphics::{
    rasterization::CullMode,
    depth_stencil::{
        DepthState,
        CompareOp
    }
};

use nalgebra::{Vector2, Vector3};

use yanyaengine::{
    game_object::*,
    ShadersContainer,
    Transform,
    ShaderId,
    Shader,
    ShadersGroup,
    Object,
    ObjectInfo,
    DefaultModel,
    Control,
    App,
    YanyaApp,
    camera::Camera
};

use stephanie::{
    server::world::world_generator::{
        ChunkGenerator,
        ConditionalInfo
    },
    client::game_state::{
        ControlsController,
        ui::controller::*
    },
    common::{
        render_info::*,
        lisp::*,
        TileMap,
        colors::Lcha,
        world::CHUNK_SIZE
    }
};


mod ui_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/ui.vert"
    }
}

mod ui_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/ui.frag"
    }
}

mod textured_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/textured.frag"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UiId
{
    Screen,
    ScreenBody,
    Scrollbar(UiScrollbarId, UiScrollbarPart),
    NameBody,
    Name,
    Padding(u32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UiScrollbarId
{
    Height,
    Difficulty
}

const DIFFICULTY_MAX: f32 = 5.0;

impl UiScrollbarId
{
    fn from_f32(&self, tags: &mut Tags, value: f32)
    {
        match self
        {
            Self::Height =>
            {
                let top = CHUNK_SIZE as i32 - 1;

                tags.height = ((value * top as f32).floor() as i32).clamp(0, top);
            },
            Self::Difficulty =>
            {
                tags.difficulty = value * DIFFICULTY_MAX;
            }
        }
    }

    fn to_f32(&self, tags: &Tags) -> f32
    {
        match self
        {
            Self::Height =>
            {
                let top = CHUNK_SIZE as i32 - 1;

                tags.height as f32 / top as f32
            },
            Self::Difficulty =>
            {
                tags.difficulty / DIFFICULTY_MAX
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UiScrollbarPart
{
    Panel,
    Text,
    Body,
    Bar
}

impl Idable for UiId
{
    fn screen() -> Self { Self::Screen }

    fn padding(id: u32) -> Self { Self::Padding(id) }
}

fn new_tile(
    info: &ObjectCreatePartialInfo,
    name: &str,
    pos: Vector2<usize>
) -> Object
{
    let assets = info.assets.lock();

    let model_id = assets.default_model(DefaultModel::Square);
    let model = assets.model(model_id).clone();

    let texture = assets.texture_by_name(&format!("tiles/{name}.png")).clone();

    let total_size = 0.5;
    let tile_size = total_size / CHUNK_SIZE as f32;

    let pos = Vector2::repeat((-total_size + tile_size) * 0.5) + pos.cast() * tile_size;

    let position = Vector3::new(pos.x, pos.y, 0.0);

    let object_info = ObjectInfo{
        model,
        texture,
        transform: Transform{
            position,
            scale: Vector3::repeat(tile_size),
            ..Default::default()
        }
    };

    info.object_factory.create(object_info)
}

struct ChunkPreview
{
    tiles: Vec<Object>
}

#[derive(Debug, Clone, PartialEq)]
struct Tags
{
    name: String,
    height: i32,
    difficulty: f32
}

struct ChunkPreviewer
{
    shaders: DrawShaders,
    tilemap: TileMap,
    memory: LispMemory,
    controls: ControlsController<UiId>,
    camera: Camera,
    controller: Controller<UiId>,
    update_timer: f32,
    chunk_code: Option<Lisp>,
    current_tags: Tags,
    preview_tags: Tags,
    preview: Option<ChunkPreview>
}

impl ChunkPreviewer
{
    fn compile_chunk(&mut self)
    {
        let parent_directory = PathBuf::from("world_generation");
        let filepath = parent_directory.join("chunks").join(format!("{}.scm", &self.preview_tags.name));

        if !filepath.exists()
        {
            self.chunk_code = None;
            return;
        }

        let standard_path = "lisp/standard.scm";
        let standard_code = fs::read_to_string(standard_path).unwrap_or_else(|err|
        {
            panic!("cant load {standard_path}: {err}")
        });

        let default_path = parent_directory.join("default.scm");
        let default_code = fs::read_to_string(&default_path).unwrap_or_else(|err|
        {
            panic!("cant load {}: {err}", default_path.display())
        });

        let chunk_code = fs::read_to_string(&filepath).unwrap_or_else(|err|
        {
            panic!("cant load {}: {err}", filepath.display())
        });

        let config = LispConfig{
            type_checks: true,
            memory: self.memory.clone()
        };

        match Lisp::new_with_config(config, &[&standard_code, &default_code, &chunk_code])
        {
            Ok(lisp) => self.chunk_code = Some(lisp),
            Err(_err) => ()
        }
    }
}

struct DrawShaders
{
    normal: ShaderId,
    ui: ShaderId
}

impl YanyaApp for ChunkPreviewer
{
    type AppInfo = Option<DrawShaders>;

    fn init(info: InitPartialInfo, app_info: Self::AppInfo) -> Self
    {
        let tilemap = TileMap::parse("tiles/tiles.json", "textures/tiles/").unwrap_or_else(|err|
        {
            panic!("error creating tilemap: {err}")
        }).tilemap;

        let primitives = Rc::new(ChunkGenerator::default_primitives(&tilemap));

        let memory = LispMemory::new(primitives, 256, 1 << 13);

        let controls = ControlsController::new();

        let camera = Camera::new(info.aspect(), -1.0..1.0);

        let controller = Controller::new(&info);

        let tags = Tags{name: String::new(), height: 1, difficulty: 0.0};

        let preview = None;

        Self{
            shaders: app_info.unwrap(),
            tilemap,
            memory,
            controls,
            camera,
            controller,
            update_timer: 0.0,
            chunk_code: None,
            current_tags: tags.clone(),
            preview_tags: tags,
            preview
        }
    }

    fn update(&mut self, partial_info: UpdateBuffersPartialInfo, dt: f32)
    {
        let mut info = partial_info.to_full(&self.camera);

        let mut controls = self.controls.changed_this_frame();

        {
            let controls = &mut controls;

            let aspect = self.camera.aspect();
            let screen_body = self.controller.update(UiId::ScreenBody, UiElement{
                children_layout: UiLayout::Vertical,
                width: aspect.min(1.0).into(),
                height: aspect.recip().min(1.0).into(),
                ..Default::default()
            });

            let mut update_scrollbar = |this_id, tags: &mut Tags|
            {
                let id = |part_id|
                {
                    UiId::Scrollbar(this_id, part_id)
                };

                let panel = screen_body.update(id(UiScrollbarPart::Panel), UiElement{
                    width: UiElementSize{
                        minimum_size: Some(UiMinimumSize::FitChildren),
                        size: UiSize::Rest(1.0)
                    },
                    children_layout: UiLayout::Horizontal,
                    ..Default::default()
                });

                let scrollbar_id = id(UiScrollbarPart::Body);
                let body = panel.update(scrollbar_id, UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 0.5})),
                    width: UiSize::Pixels(250.0).into(),
                    height: UiSize::Pixels(30.0).into(),
                    children_layout: UiLayout::Horizontal,
                    ..Default::default()
                });

                let bar_width = 0.1;

                let is_horizontal = true;
                if let Some(value) = scrollbar_handle(
                    controls,
                    body,
                    &scrollbar_id,
                    bar_width,
                    is_horizontal,
                    false
                )
                {
                    this_id.from_f32(tags, value);
                }

                let scroll = this_id.to_f32(tags);
                add_padding_horizontal(body, UiSize::Rest(scroll).into());
                let bar = body.update(id(UiScrollbarPart::Bar), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 0.5})),
                    width: UiSize::CopyElement(UiDirection::Horizontal, bar_width, scrollbar_id).into(),
                    height: UiSize::Rest(1.0).into(),
                    ..Default::default()
                });
                add_padding_horizontal(body, UiSize::Rest(1.0 - scroll).into());

                if bar.is_mouse_inside() || controls.observe_action_held(&scrollbar_id)
                {
                    bar.element().mix = Some(MixColorLch::color(Lcha{l: 40.0, c: 0.0, h: 0.0, a: 0.5}));
                }

                let description = match this_id
                {
                    UiScrollbarId::Height =>
                    {
                        format!("height: {}", tags.height)
                    },
                    UiScrollbarId::Difficulty =>
                    {
                        format!("difficulty: {:.2}", tags.difficulty)
                    }
                };

                add_padding_horizontal(panel, UiSize::Pixels(30.0).into());
                panel.update(id(UiScrollbarPart::Text), UiElement{
                    texture: UiTexture::Text{text: description, font_size: 20},
                    ..UiElement::fit_content()
                });
            };

            update_scrollbar(UiScrollbarId::Height, &mut self.current_tags);

            add_padding_vertical(screen_body, UiSize::Pixels(10.0).into());

            update_scrollbar(UiScrollbarId::Difficulty, &mut self.current_tags);

            add_padding_vertical(screen_body, UiSize::Rest(1.0).into());

            let name_body = screen_body.update(UiId::NameBody, UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 0.5})),
                width: UiElementSize{minimum_size: Some(UiMinimumSize::Pixels(250.0)), size: UiSize::FitChildren},
                height: UiSize::Pixels(50.0).into(),
                ..Default::default()
            });

            add_padding_horizontal(name_body, UiSize::Pixels(10.0).into());

            let name = &mut self.current_tags.name;

            text_input_handle(controls, name);

            name_body.update(UiId::Name, UiElement{
                texture: UiTexture::Text{text: name.clone(), font_size: 20},
                ..UiElement::fit_content()
            });

            add_padding_horizontal(name_body, UiSize::Pixels(10.0).into());
        }

        self.controller.create_renders(&mut info, dt);

        let recreate_preview = self.update_timer <= 0.0 && (self.preview.is_none() || self.current_tags != self.preview_tags);

        if recreate_preview
        {
            self.preview_tags = self.current_tags.clone();

            self.compile_chunk();

            if let Some(chunk_code) = self.chunk_code.as_mut()
            {
                let chunk_info = ConditionalInfo{
                    height: self.preview_tags.height,
                    difficulty: self.preview_tags.difficulty,
                    tags: &[]
                };

                let tiles = ChunkGenerator::generate_chunk_with(
                    &chunk_info,
                    chunk_code,
                    &self.preview_tags.name,
                    &mut |_marker|
                    {
                    }
                );

                self.preview = Some(ChunkPreview{
                    tiles: tiles.flat_slice_iter(0).filter_map(|(pos, tile)|
                    {
                        let pos = pos.pos;

                        if tile.is_none()
                        {
                            return None;
                        }

                        let name = &self.tilemap.info(*tile).name;

                        Some(new_tile(&info.partial, name, Vector2::new(pos.x, pos.y)))
                    }).collect()
                });
            }

            self.update_timer = 0.5;
        }

        self.update_timer -= dt;

        if let Some(preview) = self.preview.as_mut()
        {
            preview.tiles.iter_mut().for_each(|x| x.update_buffers(&mut info));
        }

        self.controller.update_buffers(&mut info);

        self.controls.consume_changed(controls).for_each(drop);
    }

    fn input(&mut self, control: Control)
    {
        self.controls.handle_input(control);
    }

    fn mouse_move(&mut self, (x, y): (f64, f64))
    {
        let normalized_size = self.camera.normalized_size();
        let position = Vector2::new(x as f32, y as f32).component_mul(&normalized_size) - normalized_size * 0.5;
        self.controller.set_mouse_position(position);
    }

    fn draw(&mut self, mut info: DrawInfo)
    {
        if let Some(preview) = self.preview.as_ref()
        {
            info.bind_pipeline(self.shaders.normal);

            preview.tiles.iter().for_each(|x| x.draw(&mut info));
        }

        info.bind_pipeline(self.shaders.ui);

        self.controller.draw(&mut info);
    }

    fn resize(&mut self, aspect: f32)
    {
        self.camera.resize(aspect);
    }
}

fn main()
{
    let mut shaders = ShadersContainer::new();

    let normal = shaders.push(Shader{
        shader: ShadersGroup::new(
            ui_vertex::load,
            textured_fragment::load
        ),
        depth: Some(DepthState{
            write_enable: true,
            compare_op: CompareOp::Always
        }),
        per_vertex: Some(vec![Object::per_vertex()]),
        subpass: 0,
        cull: CullMode::None,
        ..Default::default()
    });

    let ui = shaders.push(Shader{
        shader: ShadersGroup::new(
            ui_vertex::load,
            ui_fragment::load
        ),
        depth: Some(DepthState{
            write_enable: true,
            compare_op: CompareOp::Always
        }),
        per_vertex: Some(vec![Object::per_vertex()]),
        subpass: 0,
        cull: CullMode::None,
        ..Default::default()
    });

    App::<ChunkPreviewer>::new()
        .with_title("chunk preview")
        .with_textures_path("textures")
        .with_shaders(shaders)
        .with_app_init(Some(DrawShaders{normal, ui}))
        .with_clear_color([0.4, 0.4, 0.45])
        .run();
}
