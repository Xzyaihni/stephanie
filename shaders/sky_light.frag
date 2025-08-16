#version 450

layout(location = 0) in float out_intensity;

layout(location = 0) out vec4 f_color;

layout(push_constant) uniform BackgroundColor{
    vec3 color;
} background_color;

void main()
{
    f_color = vec4(background_color.color * pow(out_intensity, 2), 1.0);
}
