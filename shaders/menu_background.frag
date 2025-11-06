#version 450

// v this could be a driver bug or some peak brain damaged design by the worst people on earth (gpu designers) v

// float coord = ((gl_FragCoord.x > 100000) ? gl_FragCoord.x : 25.0) + ((gl_FragCoord.y > 100000) ? gl_FragCoord.y : 0.0);
// float outline_lines = mod(mod(coord * 0.64, 2.0), 1.0); <-- returns >1 when running the program, renderdoc gives 0

// leaning towards driver bug lol, but the spec says mod is very inaccurate so i guess i dont get to math

layout(location = 0) out vec4 f_color;

layout(push_constant) uniform BackgroundInfo{
    float animation;
} info;

const float LINE_WIDTH = 4.0;

const vec3 COLOR_ONE = vec3(0.9352931696328095,0.49040246819760785,0.7407528502700176);
const vec3 COLOR_TWO = vec3(0.9287431375377354,0.43808454459382157,0.7124316972334828);

// doing some bespoke math to try to calculate the gaussian blur without convolutions
const float SDEV = 0.05;
const int ITERATIONS = 10;

void main()
{
    float coord = (gl_FragCoord.x + gl_FragCoord.y) * 0.02 + info.animation * 2.0 * LINE_WIDTH;
    float d = distance(mod(coord, LINE_WIDTH) / LINE_WIDTH, 0.5) * 2.0;

    float z = -(sqrt(d * d * 2) * 0.5) / SDEV;

    float a = 0.0;

    for(int i = 0; i < ITERATIONS; ++i)
    {
        float s = 6.0 / ITERATIONS;
        float t = z - (s * i + s * 0.5);

        float value = exp(-(t * t) / 2.0);

        a += value * s;
    }

    float amount = a / sqrt(2.0 * 3.1415926535);

    vec3 outline_mask = mix(COLOR_ONE, COLOR_TWO, int((coord + LINE_WIDTH / 2.0) / LINE_WIDTH) % 2 == 0 ? amount : (1.0 - amount));

    f_color = vec4(outline_mask, 1.0);
}
