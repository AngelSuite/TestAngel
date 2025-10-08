fn main() {
    if cfg!(feature = "cli") || cfg!(feature = "ui") {
        println!("cargo::rerun-if-changed=../icon.png");

        relm4_icons_build::bundle_icons(
            // Name of the file that will be generated at `OUT_DIR`
            "icon_names.rs",
            // Optional app ID
            Some("uk.hpkns.testangel"),
            // Custom base resource path:
            // * defaults to `/com/example/myapp` in this case if not specified explicitly
            // * or `/org/relm4` if app ID was not specified either
            None::<&str>,
            // Directory with custom icons (if any)
            Some("icons"),
            // List of icons to include
            [
                "down",
                "edit",
                "lightbulb",
                "menu",
                "papyrus-vertical",
                "play",
                "plus",
                "puzzle-piece",
                "settings",
                "tag",
                "up",
                "cross-small-circle-filled",
            ],
        );

        #[cfg(windows)]
        {
            ico_builder::IcoBuilder::default()
                .add_source_file("../icon.png")
                .build_file("../icon.ico")
                .unwrap();

            let mut res = winres::WindowsResource::new();
            res.set_icon("../icon.ico");
            res.compile().unwrap();
        }
    }
}
