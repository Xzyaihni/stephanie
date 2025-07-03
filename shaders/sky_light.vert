#version 450

layout(location = 0) in vec2 position;
layout(location = 1) in float intensity;

layout(location = 0) out float out_intensity;

void main()
{
    gl_Position = vec4(position, 0.0, 1.0);

    out_intensity = intensity;
}
