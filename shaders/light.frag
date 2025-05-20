#version 450

layout(location = 0) in vec2 tex_coords;

layout(location = 0) out vec4 f_color;

void main()
{
    vec2 d = tex_coords - 0.5;
    f_color = vec4(vec3(1.0), clamp(sqrt((d.x * d.x) + (d.y * d.y)) * 2.0, 0.0, 1.0));
}
