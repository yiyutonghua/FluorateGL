use fluorategl::shader_translator::preprocess;
fn main() {
    let src = "#version 330 core\nuniform sampler2D tex;\nin vec2 uv;\nout vec4 fragColor;\nvoid main() { fragColor = texture(tex, uv); }\n";
    let result = preprocess::preprocess(src, 0x8B31);
    println!("{}", result);
}
