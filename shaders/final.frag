#version 450

layout(input_attachment_index = 0, set = 0, binding = 0) uniform subpassInput color;
layout(input_attachment_index = 1, set = 0, binding = 1) uniform subpassInput shaded;
layout(input_attachment_index = 2, set = 0, binding = 2) uniform subpassInput lighting;

layout(location = 0) out vec4 f_color;

void main()
{
    vec4 light = subpassLoad(lighting).rgba;
    float a = light.a;
    f_color = mix(subpassLoad(color).rgba * vec4(light.rgb, 1.0), subpassLoad(shaded).rgba, a);
}
