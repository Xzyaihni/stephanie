#version 450

layout(location = 0) in float out_intensity;

layout(location = 0) out vec4 f_color;

void main()
{
    f_color = vec4(vec3(1.0) * pow(out_intensity, 2), 1.0);
}
