//! Offline validation of the engine's WGSL shaders — parse + full naga validation, no GPU needed.
//! wgpu only surfaces WGSL errors at pipeline-creation time (runtime, on a real device), so these
//! tests are the headless safety net that keeps the sky/post/geometry shaders from regressing.

use naga::valid::{Capabilities, ValidationFlags, Validator};

fn validate(name: &str, src: &str) {
    let module = match naga::front::wgsl::parse_str(src) {
        Ok(m) => m,
        Err(e) => panic!("{name}: WGSL parse error:\n{}", e.emit_to_string(src)),
    };
    let mut v = Validator::new(ValidationFlags::all(), Capabilities::all());
    if let Err(e) = v.validate(&module) {
        panic!("{name}: WGSL validation error: {e:?}");
    }
}

#[test]
fn sky_shader_valid() {
    validate("sky.wgsl", include_str!("../src/sky.wgsl"));
}

#[test]
fn post_shader_valid() {
    validate("post.wgsl", include_str!("../src/post.wgsl"));
}

#[test]
fn geometry_shader_valid() {
    validate("shader.wgsl", include_str!("../src/shader.wgsl"));
}

#[test]
fn loading_shader_valid() {
    validate("loading.wgsl", include_str!("../src/loading.wgsl"));
}

/// The post shader declares three fragment entry points sharing one uniform + one 2-texture group.
/// Assert all four (vs + 3 fs) entry points are present so the pipelines can be built.
#[test]
fn post_shader_has_expected_entry_points() {
    let module = naga::front::wgsl::parse_str(include_str!("../src/post.wgsl")).expect("parse post.wgsl");
    let names: Vec<&str> = module.entry_points.iter().map(|e| e.name.as_str()).collect();
    for want in ["vs_main", "fs_bright", "fs_blur", "fs_composite"] {
        assert!(names.contains(&want), "post.wgsl missing entry point {want}; has {names:?}");
    }
}
