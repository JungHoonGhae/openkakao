fn main() {
    println!("cargo:rerun-if-changed=native/macos_user_presence.m");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("native/macos_user_presence.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("openkakao_user_presence");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=LocalAuthentication");
        println!("cargo:rustc-link-lib=framework=Security");
    }
}
