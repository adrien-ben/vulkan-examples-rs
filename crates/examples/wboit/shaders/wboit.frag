#version 450

layout(location = 0) in vec4 oColor;

layout(location = 0) out vec4 weightedColor;
layout(location = 1) out float reveal;

void main() {

    // weight function
    float weight = clamp(pow(min(1.0, oColor.a * 10.0) + 0.01, 3.0) * 1e8 * 
                         pow(1.0 - gl_FragCoord.z * 0.9, 3.0), 1e-2, 3e3);

    // float weight =
    // max(min(1.0, max(max(oColor.r, oColor.g), oColor.b) * oColor.a), oColor.a) *
    // clamp(0.03 / (1e-5 + pow(gl_FragCoord.z / 200, 4.0)), 1e-2, 3e3);

    // store pixel color accumulation
    weightedColor = vec4(oColor.rgb * oColor.a, oColor.a) * weight;

    // store pixel revealage threshold
    reveal = oColor.a;
}
