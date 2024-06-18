#version 450

layout(location = 0) out vec4 f_color;

const vec3 shadow_color = vec3(0.07, 0.02, 0.1);

void main()
{
    f_color = vec4(shadow_color, 1.0);
}
