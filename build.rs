// Qt Quick interface of Conversations, compiled into the binary (module Fili.Im, qrc:/qt/qml/Fili/Im/qml/); the Rust
// object it talks to is in src/gui/bridge.rs.
#[cfg(feature = "gui")]
use cxx_qt_build::{CxxQtBuilder, QmlModule};

#[cfg(not(feature = "gui"))]
fn main() {}

#[cfg(feature = "gui")]
fn main() {
    // Tokens of the shared kit (Fili.UI), for the test that checks the FiliTheme.* names used in the QML
    if let Ok(p) = std::env::var("DEP_FILI_UI_THEME_QML") {
        println!("cargo::rustc-env=FILI_UI_THEME_QML={p}");
    }
    // Version of the About card: tag or commit of the repository (absent outside a repository)
    if let Ok(o) = std::process::Command::new("git").args(["describe", "--tags", "--always", "--dirty"]).output()
        && o.status.success()
    {
        println!("cargo:rustc-env=FILI_IM_GIT={}", String::from_utf8_lossy(&o.stdout).trim());
    }
    CxxQtBuilder::new_qml_module(
        QmlModule::new("Fili.Im")
            .qml_files([
                "qml/Main.qml",
                "qml/Sidebar.qml",
                "qml/ChatList.qml",
                "qml/Conversation.qml",
                "qml/Bubble.qml",
                "qml/Composer.qml",
                "qml/PairDialog.qml",
                "qml/AboutCard.qml",
                "qml/ShortcutsCard.qml",
                "qml/SwitchCard.qml",
                "qml/EmojiPicker.qml",
                "qml/QuickLook.qml",
                "qml/Avatar.qml",
                "qml/VideoLink.qml",
                "qml/ContactCard.qml",
            ])
            .depend("Fili.UI")
            .depend("QtQuick")
            .depend("QtQuick.Controls")
            .depend("QtQuick.Dialogs")
            .depend("QtMultimedia")
            .depend("QtQuick.Effects"),
    )
    // Fonts of the interface travel with the binary: the application looks the same on any distribution
    .qrc_resources([
        "fonts/Jost-Variable.ttf",
        "fonts/JetBrainsMono-Variable.ttf",
        "fonts/FiliIcons.ttf",
        // The icon of the application, shown by the About card
        "data/icons/hicolor/scalable/apps/fili-im.svg",
    ])
    .qt_module("Quick")
    .files(["src/gui/bridge.rs"])
    .build();
}
