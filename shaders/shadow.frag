#version 450

layout(location = 0) out vec4 f_color;

layout(location = 0) in float depth;

const vec3 shadow_color = vec3(0.07, 0.02, 0.1);

const vec3 background_color = vec3(0.831, 0.941, 0.988);

void main()
{
    f_color = vec4(mix(shadow_color, background_color, depth), 1.0);
}
