#!/bin/bash

find ./crates/examples/*/shaders -not -name *.spv -type f -exec glslangValidator --target-env spirv1.6 -V -o {}.spv {} \;
