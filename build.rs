fn main() {
    // The Slint UI is only compiled for the GUI build.
    #[cfg(feature = "gui")]
    slint_build::compile("ui/installer.slint").expect("Slint build failed");
}
