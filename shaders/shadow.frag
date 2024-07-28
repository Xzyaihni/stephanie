#version 450

layout(location = 0) out vec4 f_color;

const vec3 shadow_color = vec3(0.07, 0.02, 0.1);

void main()
{
    vec3 color = shadow_color;

    f_color = vec4(color, 1.0);
}
