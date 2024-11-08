#version 450

layout(location = 0) out vec4 f_color;

const vec3 background_color = vec3(0.831, 0.941, 0.988);

void main()
{
    f_color = vec4(background_color, 1.0);
}
