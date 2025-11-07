#version 450

layout(location = 0) in vec2 tex_coords;

layout(location = 0) out vec4 f_color;

layout(push_constant) uniform MouseInfo{
    float amount;
} info;

const vec3 COLOR_FULL = vec3(1.0, 0.766, 0.953);
const vec3 COLOR_EMPTY = vec3(0.375, 0.258, 0.322);

const float THICKNESS = 0.3;

const float PI = 3.1415926535;

void main()
{
    float angle = atan(tex_coords.x - 0.5, tex_coords.y - 0.5) + PI;
    float angle_fraction = angle / (2.0 * PI);
    vec3 circle_color = angle_fraction > info.amount ? COLOR_FULL : COLOR_EMPTY;

    float d = distance(tex_coords, vec2(0.5));

    f_color = (d <= 0.5) && (d >= (0.5 - THICKNESS)) ? vec4(circle_color, 1.0) : vec4(0.0);
}
