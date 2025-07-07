use nalgebra::Vector3;

use yanyaengine::{
    game_object::*,
    Transform,
    ShaderId,
    Object,
    ObjectInfo,
    DefaultModel,
    Control,
    App,
    YanyaApp,
    camera::Camera
};

use stephanie::client::game_state::ui::controller::*;


#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum UiId
{
    Screen,
    Padding(u32)
}

impl Idable for UiId
{
    fn screen() -> Self { Self::Screen }

    fn padding(id: u32) -> Self { Self::Padding(id) }
}

fn new_tile(
    info: &ObjectCreatePartialInfo,
    name: &str
) -> Object
{
    let assets = info.assets.lock();

    let model_id = assets.default_model(DefaultModel::Square);
    let model = assets.model(model_id).clone();

    let texture = assets.texture_by_name(&format!("tiles/{name}.png")).clone();

    let object_info = ObjectInfo{
        model,
        texture,
        transform: Transform{
            scale: Vector3::repeat(0.5),
            ..Default::default()
        }
    };

    info.object_factory.create(object_info)
}

struct ChunkPreview
{
    tiles: Object
}

struct Tags
{
    height: i32,
    difficulty: f32
}

struct ChunkPreviewer
{
    camera: Camera,
    controller: Controller<UiId>,
    tags: Tags,
    preview: Option<ChunkPreview>
}

impl YanyaApp for ChunkPreviewer
{
    type AppInfo = ();

    fn init(info: InitPartialInfo, _app_info: Self::AppInfo) -> Self
    {
        let camera = Camera::new(info.aspect(), -1.0..1.0);

        let controller = Controller::new(&info);

        let tags = Tags{height: 1, difficulty: 0.0};

        let preview = None;

        Self{camera, controller, tags, preview}
    }

    fn update(&mut self, partial_info: UpdateBuffersPartialInfo, dt: f32)
    {
        let mut info = partial_info.to_full(&self.camera);

        self.controller.create_renders(&mut info, dt);

        let recreate_preview = self.preview.is_none();

        if recreate_preview
        {
            self.preview = Some(ChunkPreview{
                tiles: new_tile(&info.partial, "soil")
            });
        }

        if let Some(preview) = self.preview.as_mut()
        {
            preview.tiles.update_buffers(&mut info);
        }

        self.controller.update_buffers(&mut info);
    }

    fn input(&mut self, control: Control)
    {
    }

    fn mouse_move(&mut self, position: (f64, f64))
    {
    }

    fn draw(&mut self, mut info: DrawInfo)
    {
        if let Some(preview) = self.preview.as_ref()
        {
            info.bind_pipeline(ShaderId::default());

            preview.tiles.draw(&mut info);
        }

        self.controller.draw(&mut info);
    }

    fn resize(&mut self, aspect: f32)
    {
        self.camera.resize(aspect);
    }
}

fn main()
{
    App::<ChunkPreviewer>::new()
        .with_title("chunk preview")
        .with_textures_path("textures")
        .run();
}
