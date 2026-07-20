#version 330 core
in vec3 Position;
in vec2 UV;
out vec2 vUV;
uniform mat4 ModelViewProjection;
void main() {
    vUV = UV;
    gl_Position = ModelViewProjection * vec4(Position, 1.0);
}
