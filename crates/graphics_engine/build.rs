use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn discover_module_files(modules_dir: &Path) -> Vec<PathBuf> {
    let mut module_files = Vec::new();

    if let Ok(entries) = fs::read_dir(modules_dir) {
        entries.flatten().for_each(|entry| {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "slang") {
                module_files.push(path);
            }
        });
    }

    module_files
}

fn discover_pass_files(passes_dir: &Path) -> Vec<PathBuf> {
    let mut pass_files = Vec::new();

    if let Ok(entries) = fs::read_dir(passes_dir) {
        entries.flatten().for_each(|entry| {
            let path = entry.path();
            if path.is_dir()
                && let Ok(pass_entries) = fs::read_dir(&path)
            {
                pass_entries.flatten().for_each(|pass_entry| {
                    let pass_file = pass_entry.path();
                    if pass_file.is_file()
                        && pass_file.extension().is_some_and(|ext| ext == "slang")
                    {
                        pass_files.push(pass_file);
                    }
                });
            }
        });
    }

    pass_files
}

fn compile_module(module_file: &Path, output_dir: &Path) -> bool {
    let base_name = module_file.file_stem().unwrap().to_str().unwrap();
    let output_file = output_dir.join(format!("{base_name}.slang-module"));

    let mut cmd = Command::new("slangc");
    cmd.arg("-o").arg(&output_file).arg(module_file);

    let output = cmd.output();

    match output {
        Ok(result) => {
            if !result.status.success() {
                println!("cargo:warning={}", String::from_utf8_lossy(&result.stderr));
                false
            } else {
                true
            }
        }
        Err(error) => {
            println!("cargo:warning={error}");
            false
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn compile_shader(shader_file: &Path, output_dir: &Path, modules_dir: &Path) -> bool {
    let base_name = shader_file.file_stem().unwrap().to_str().unwrap();
    let output_file = output_dir.join(format!("{base_name}.spv"));

    let mut cmd = Command::new("slangc");

    cmd.arg("-target")
        .arg("spirv")
        .arg("-O3")
        .arg("-I")
        .arg(modules_dir)
        .arg("-profile")
        .arg("spirv_1_6")
        // Uses column major layout for matrices.
        .arg("-matrix-layout-column-major")
        // Uses the entrypoint name from the source instead of 'main' in the spirv output.
        .arg("-fvk-use-entrypoint-name")
        // Make data accessed through ConstantBuffer, ParameterBlock, StructuredBuffer,
        // ByteAddressBuffer and general pointers follow the 'scalar' layout.
        .arg("-fvk-use-scalar-layout");

    cmd.arg("-o").arg(&output_file).arg(shader_file);

    let output = cmd.output();

    match output {
        Ok(result) => {
            if !result.status.success() {
                println!("cargo:warning={}", String::from_utf8_lossy(&result.stderr));
                false
            } else {
                true
            }
        }
        Err(error) => {
            println!("cargo:warning={error}");
            false
        }
    }
}

fn main() {
    check_slangc_availability();

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let manifest_path = PathBuf::from(manifest_dir);
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");

    let shader_dir = manifest_path.join("shaders");
    let modules_dir = shader_dir.join("modules");
    let passes_dir = shader_dir.join("passes");
    let output_dir = PathBuf::from(out_dir).join("shaders_compiled");
    let modules_output_dir = output_dir.join("modules");
    let passes_output_dir = output_dir.join("passes");

    if output_dir.exists() {
        fs::remove_dir_all(&output_dir).expect("failed to remove output directory");
    }
    fs::create_dir_all(&output_dir).expect("failed to create output directory");
    fs::create_dir_all(&modules_output_dir).expect("failed to create modules output directory");
    fs::create_dir_all(&passes_output_dir).expect("failed to create passes output directory");

    let mut had_error = false;

    let module_files = discover_module_files(&modules_dir);
    let pass_files = discover_pass_files(&passes_dir);

    println!("cargo:rerun-if-changed=build.rs");
    for module_file in &module_files {
        println!("cargo:rerun-if-changed={}", module_file.display());
    }
    for pass_file in &pass_files {
        println!("cargo:rerun-if-changed={}", pass_file.display());
    }

    for module_file in &module_files {
        let success = compile_module(module_file, &modules_output_dir);
        if !success {
            had_error = true;
        }
    }
    for pass_file in &pass_files {
        let pass_subdirectory = pass_file
            .parent()
            .unwrap()
            .strip_prefix(&passes_dir)
            .unwrap_or(Path::new(""));
        let subdirectory = passes_output_dir.join(pass_subdirectory);

        fs::create_dir_all(&subdirectory).expect("failed to create pass output subdirectory");

        let success = compile_shader(pass_file, &subdirectory, &modules_dir);

        if !success {
            had_error = true;
        }
    }

    if had_error {
        std::process::exit(1);
    }
}

fn check_slangc_availability() {
    const MIN_YEAR: u32 = 2026;
    const MIN_MAJOR: u32 = 1;

    let result = Command::new("slangc").arg("-version").output();

    match result {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("Error: slangc is installed but failed to report version");
                std::process::exit(1);
            }

            // `slangc` currently outputs all diagnostics messages to `stderr`, including the version.
            let version_output = String::from_utf8_lossy(&output.stderr);

            match parse_slangc_version(&version_output) {
                Some((year, major)) => {
                    let is_valid = year > MIN_YEAR
                        || (year == MIN_YEAR && major > MIN_MAJOR)
                        || (year == MIN_YEAR && major == MIN_MAJOR);

                    if !is_valid {
                        eprintln!("Error: slangc version {year}.{major} is too old.");
                        eprintln!(
                            "At least version {MIN_YEAR}.{MIN_MAJOR} is required to compile shaders."
                        );
                        eprintln!("Please update slangc by:");
                        eprintln!(
                            "  1. Downloading slang directly from https://github.com/shader-slang/slang/releases"
                        );
                        eprintln!("  2. Installing the Vulkan SDK from https://vulkan.lunarg.com/");
                        std::process::exit(1);
                    }
                }
                None => {
                    eprintln!("Warning: Failed to parse slangc version from output:");
                    eprintln!("{version_output}");
                    eprintln!(
                        "Proceeding anyway, but at least version {MIN_YEAR}.{MIN_MAJOR} is required.",
                    );
                }
            }
        }
        Err(_) => {
            eprintln!("Error: slangc is not available in PATH.");
            eprintln!("slangc is required to compile shaders. You can install it by:");
            eprintln!(
                "  1. Downloading slang directly from https://github.com/shader-slang/slang/releases"
            );
            eprintln!("  2. Installing the Vulkan SDK from https://vulkan.lunarg.com/");
            eprintln!("After installation, ensure slangc is in your PATH.");
            eprintln!("At least version {MIN_YEAR}.{MIN_MAJOR} is needed to compile shaders.",);
            std::process::exit(1);
        }
    }
}

fn parse_slangc_version(version_output: &str) -> Option<(u32, u32)> {
    // On Nix, the version is formatted as `vYEAR.MAJOR.MINOR-nixpkgs`, so we
    // sanitize the input to be in the format `YEAR.MAJOR`.
    let sanitized_output: String = version_output
        .chars()
        .filter(|character| character.is_ascii_digit() || *character == '.')
        .collect();

    let parts: Vec<&str> = sanitized_output.split('.').collect();

    if parts.len() < 2 {
        return None;
    }

    let first = parts[0].parse::<u32>().ok()?;
    let second = parts[1].parse::<u32>().ok()?;

    Some((first, second))
}
