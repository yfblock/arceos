use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn pick_compiler() -> String {
    let mut compiler = env::var("CC").unwrap_or_else(|_| "clang".to_string());
    if compiler == "clang"
        || Command::new("which")
            .arg(&compiler)
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
    {
        if Command::new("clang")
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            compiler = "clang".to_string();
        }
    }
    compiler
}

fn add_riscv_flags(build: &mut cc::Build, compiler: &str) {
    if compiler.contains("clang") {
        build
            .flag("--target=riscv64-unknown-none-elf")
            .flag("-march=rv64gc")
            .flag("-mcmodel=medany")
            .flag("-fno-pic");
    }
}

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    if !target.contains("riscv64") {
        return;
    }

    if env::var("CARGO_FEATURE_JPU_C").is_err() {
        println!("cargo:warning=JPU C driver disabled (enable feature jpu-c)");
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let c_include = manifest_dir.join("c/include");
    let compiler = pick_compiler();

    println!("cargo:rerun-if-changed=c/");

    if env::var("CARGO_FEATURE_JPU_C_STUB").is_ok() {
        let mut build = cc::Build::new();
        build
            .compiler(&compiler)
            .flag("-Os")
            .flag("-ffreestanding")
            .flag("-nostdinc")
            .include(&c_include)
            .include(manifest_dir.join("c"))
            .file(manifest_dir.join("c/arceos_jpeg_stub.c"));
        add_riscv_flags(&mut build, &compiler);
        build.compile("cvitek_jpeg");
        println!("cargo:rustc-link-lib=static=cvitek_jpeg");
        return;
    }

    let jpeg_dir = manifest_dir
        .join("../../LicheeRV-Nano-Build/u-boot-2021.10/drivers/jpeg");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    if !jpeg_dir.exists() {
        println!(
            "cargo:warning=CVitek JPEG sources not found at {}",
            jpeg_dir.display()
        );
        return;
    }

    println!("cargo:rerun-if-changed={}", jpeg_dir.display());

    let jdi_osal_src = fs::read_to_string(jpeg_dir.join("jdi_osal.c")).expect("read jdi_osal.c");
    let jdi_osal_patched = jdi_osal_src
        .replace(
            "#define MAX_MALLOC_BLOCK_SIZE   0x50000",
            "#define MAX_MALLOC_BLOCK_SIZE   0x10000",
        )
        .replace(
            "#define MAX_MALLOC_BLOCK_NUM    7",
            "#define MAX_MALLOC_BLOCK_NUM    2",
        );
    let jdi_osal_out = out_dir.join("jdi_osal_patched.c");
    fs::write(&jdi_osal_out, jdi_osal_patched).expect("write jdi_osal_patched.c");

    let sources = [
        "mm.c",
        "jdi.c",
        "jpuapifunc.c",
        "jpuhelper.c",
        "jpuapi.c",
        "jpurun.c",
        "mixer.c",
        "jpeg.c",
    ];

    let mut build = cc::Build::new();
    build
        .compiler(&compiler)
        .flag("-Os")
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .flag("-ffreestanding")
        .flag("-fno-builtin")
        .flag("-nostdinc")
        .flag("-D__linux__")
        .flag("-D__linux")
        .flag("-Dlinux")
        .flag("-DPLATFORM_NON_OS")
        .flag("-UPLATFORM_LINUX")
        .flag("-Wno-unused-parameter")
        .flag("-Wno-unused-variable")
        .flag("-Wno-implicit-function-declaration")
        .flag("-Wno-int-conversion")
        .flag("-Wno-int-to-pointer-cast")
        .flag("-Wno-self-assign")
        .include(&c_include)
        .include(manifest_dir.join("c"))
        .include(&jpeg_dir)
        .file(manifest_dir.join("c/arceos_port.c"))
        .file(manifest_dir.join("c/arceos_jpeg.c"))
        .file(&jdi_osal_out);

    add_riscv_flags(&mut build, &compiler);

    for src in sources {
        build.file(jpeg_dir.join(src));
    }

    build.compile("cvitek_jpeg");
    println!("cargo:rustc-link-lib=static=cvitek_jpeg");
    println!("cargo:rustc-link-arg=--gc-sections");
}
