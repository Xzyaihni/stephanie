#version 450

layout(location = 0) in vec3 position;
layout(location = 1) in vec2 uv;

layout(location = 0) out vec2 tex_coords;
layout(location = 1) out float depth;

layout(constant_id = 0) const float TILE_SIZE = 0.0;

void main()
{
    gl_Position = vec4(position, 1.0);

    tex_coords = uv;
    depth = sqrt(max(0.0, position.z - TILE_SIZE * 2.0));
}
