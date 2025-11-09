#version 450

layout(location = 0) in vec2 tex_coords;

layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform sampler2D tex;

layout(push_constant) uniform MouseInfo{
    float amount;
} info;

const vec3 COLOR_FULL = vec3(1.0, 0.766, 0.953);
const vec3 COLOR_EMPTY = vec3(0.375, 0.258, 0.322);

const float PI = 3.1415926535;

void main()
{
    vec4 color = texture(tex, tex_coords);

    float angle = atan(tex_coords.x - 0.5, tex_coords.y - 0.5) + PI;
    float angle_fraction = angle / (2.0 * PI);
    vec3 circle_color = angle_fraction > info.amount ? COLOR_FULL : COLOR_EMPTY;

    f_color = color.rgb == vec3(0.0) ? vec4(circle_color, color.a) : vec4(0.0);
}
