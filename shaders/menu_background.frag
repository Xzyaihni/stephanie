#version 450

layout(location = 0) out vec4 f_color;

const float LINE_WIDTH = 4.0;

const vec3 COLOR_ONE = vec3(0.907, 0.557, 0.778);
const vec3 COLOR_TWO = vec3(0.843, 0.499, 0.718);

void main()
{
    float coord = gl_FragCoord.x + gl_FragCoord.y;
    float outline_lines = mod(coord * 0.02, LINE_WIDTH);
    vec3 outline_mask = outline_lines > (LINE_WIDTH * 0.5) ? COLOR_ONE : COLOR_TWO;

    f_color = vec4(outline_mask, 1.0);
}
