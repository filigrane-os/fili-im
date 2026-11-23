//! The window of Conversations: Qt Quick interface (qml/) over the daemon's control socket.
use fili_ui as _;

pub mod bridge;
pub mod i18n;
pub mod link;
pub mod theme;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

/// Open the window; returns the process exit code.
pub fn run() -> i32 {
    // Static initializers of the Qt side, named here so the linker keeps them (the package also has a library crate).
    cxx_qt::init_crate!(cxx_qt_lib);
    cxx_qt::init_crate!(fili_im);
    cxx_qt::init_qml_module!("Fili.Im");
    // Through the desktop portal, "Attach files" opens the desktop's file chooser (Casier on Filigrane). Must be set
    // before the application object exists.
    if std::env::var_os("QT_QPA_PLATFORMTHEME").is_none() {
        // SAFETY: nothing else runs yet (single thread, before Qt starts)
        unsafe { std::env::set_var("QT_QPA_PLATFORMTHEME", "xdgdesktopportal") };
    }
    // The log of the window (Debug panel, View > Debug Logs): ~/.local/state/filigrane/logs/fili-im.log; panics and QML
    // errors become incidents (fili-incident). Nothing of the messages is logged.
    crate::logs::init("fili-im", "filigrane-os/fili-im", env!("CARGO_PKG_VERSION"), crate::window_parts::incidents_on());
    crate::logs::capture_qt();
    let mut app = QGuiApplication::new();
    // Wayland app-id (niri rules, fili-im.desktop)
    QGuiApplication::set_desktop_file_name(&QString::from("fili-im"));
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/Fili/Im/qml/Main.qml"));
    }
    match app.as_mut() {
        Some(app) => app.exec(),
        None => 1,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    // A misspelled token does not fail at run time: the item just stays transparent
    #[test]
    fn every_theme_token_used_in_qml_is_defined() {
        let defined: Vec<String> = fs::read_to_string(env!("FILI_UI_THEME_QML"))
            .unwrap()
            .lines()
            .filter_map(|l| l.trim().strip_prefix("readonly property "))
            .filter_map(|l| Some(l.split(':').next()?.split_whitespace().last()?.to_string()))
            .chain(["alpha".to_string()])
            .collect();
        for entry in fs::read_dir(format!("{}/qml", env!("CARGO_MANIFEST_DIR"))).unwrap() {
            let path = entry.unwrap().path();
            let text = fs::read_to_string(&path).unwrap();
            for (i, _) in text.match_indices("FiliTheme.") {
                let name: String = text[i + 10..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                assert!(defined.contains(&name), "{}: FiliTheme.{name} is not defined", path.display());
            }
        }
    }
}
