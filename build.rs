// Qt Quick interface of Conversations, compiled into the binary (module Fili.Im, qrc:/qt/qml/Fili/Im/qml/); the Rust
// object it talks to is in src/gui/bridge.rs.
#[cfg(feature = "gui")]
use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

#[cfg(not(feature = "gui"))]
fn main() {}

#[cfg(feature = "gui")]
fn main() {
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
                "qml/TabBar.qml",
                "qml/PairDialog.qml",
                "qml/FiliScrollBar.qml",
                "qml/WindowGlyph.qml",
                "qml/WindowPanel.qml",
                "qml/KineticScroll.qml",
                "qml/AboutCard.qml",
                "qml/ShortcutsCard.qml",
                "qml/SwitchCard.qml",
                "qml/EmojiPicker.qml",
                "qml/QuickLook.qml",
                "qml/Avatar.qml",
                "qml/VideoLink.qml",
                "qml/Spinner.qml",
                "qml/SearchField.qml",
                "qml/ContactCard.qml",
                "qml/Icon.qml",
                "qml/Field.qml",
                "qml/IconButton.qml",
                "qml/TextButton.qml",
                "qml/FocusRing.qml",
                "qml/PopupMenu.qml",
                "qml/DebugPanel.qml",
                "qml/LiaButton.qml",
            ])
            .qml_file(QmlFile::from("qml/FiliTheme.qml").singleton(true))
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
