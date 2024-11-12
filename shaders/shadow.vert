#version 450

layout(location = 0) in vec4 position;

void main()
{
    gl_Position = position;
    gl_Position.z = 0.0;
}
