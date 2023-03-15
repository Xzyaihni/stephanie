#[derive(Debug)]
pub struct Model
{
    pub vertices: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>
}

#[allow(dead_code)]
impl Model
{
    pub fn new() -> Self
    {
        Self{vertices: Vec::new(), uvs: Vec::new()}
    }

    pub fn rectangle(size: f32) -> Self
    {
        let vertices = vec![
            [-size, -size, 0.0],
            [-size, size, 0.0],
            [size, -size, 0.0],
            [-size, size, 0.0],
            [size, size, 0.0],
            [size, -size, 0.0]
        ];

        let uvs = vec![
            [0.0, 0.0],
            [0.0, 1.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0]
        ];

        Self{vertices, uvs}
    }
}