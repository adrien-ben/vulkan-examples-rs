#version 450

layout(location = 0) in vec2 iUV;

layout(location = 0) out vec4 oColor;

void main() {

    float   real  = iUV.x;
    float   imag  = iUV.y;
    float   Creal = real;  
    float   Cimag = imag;  

    float r2 = 0.0;

    // Stupid number of iterations to stress gpu
    for (float iter = 0.0; iter < 10000 && r2 < 4.0; ++iter) {
        float tempreal = real;

        real = (tempreal * tempreal) - (imag * imag) + Creal;
        imag = 2.0 * tempreal * imag + Cimag;
        r2   = (real * real) + (imag * imag);
    }

    vec3 color;

    if (r2 < 4.0)
        color = vec3(0.0f, 0.0f, 0.0f);
    else
        color = vec3(1.0f, 1.0f, 1.0f);

    oColor = vec4(color, 1.0);
}
