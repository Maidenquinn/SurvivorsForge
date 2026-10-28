fn main() {
    slint_build::compile("ui/app.slint").expect("Slint UI compiles");

    // Icon and version info of the .exe (Explorer, taskbar pin, Alt+Tab before the window shows).
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("assets/icon.ico")
        .set("FileDescription", "SurvivorsForge")
        .set("ProductName", "SurvivorsForge")
        .compile()
        .expect("Windows resources compile");
}
