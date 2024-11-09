#version 450

layout(location = 0) out vec4 f_color;

layout(constant_id = 0) const float BACK_RED = 0.0;
layout(constant_id = 1) const float BACK_GREEN = 0.0;
layout(constant_id = 2) const float BACK_BLUE = 0.0;

void main()
{
    f_color = vec4(vec3(BACK_RED, BACK_GREEN, BACK_BLUE), 1.0);
}
