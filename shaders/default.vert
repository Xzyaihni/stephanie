#version 450

layout(location = 0) in vec4 position;
layout(location = 1) in vec2 uv;

layout(location = 0) out vec2 tex_coords;
layout(location = 1) out float depth;

void main()
{
    gl_Position = position;

    tex_coords = uv;
    depth = position.z;
}
