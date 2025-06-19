#version 450

layout(location = 0) in vec2 tex_coords;

layout(location = 0) out vec4 f_color;

void main()
{
    vec2 o = tex_coords - 0.5;
    float d = sqrt((o.x * o.x) + (o.y * o.y)) * 2.0;

    float cut = 0.7;
    float b = 20.0;

    float cut_brightness = 1.0 / (1.0 + b * cut * cut);

    float intensity = d > cut
        ? max((1.0 - d) / (1.0 - cut) * cut_brightness, 0.0)
        : 1.0 / (1.0 + b * d * d);

    f_color = vec4(vec3(1.0), clamp(intensity, 0.0, 1.0));
}
