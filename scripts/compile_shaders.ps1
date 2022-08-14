Get-ChildItem -Path .\crates\examples\ -File -Recurse -exclude *.spv | Where-Object {$_.fullname -Match "shaders"}  | ForEach-Object { 
    $sourcePath = $_.fullname
    $targetPath = "$($_.fullname).spv"
    glslangValidator --target-env spirv1.4 -V -o $targetPath $sourcePath
}
