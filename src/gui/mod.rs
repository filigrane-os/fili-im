//! The window of Conversations: Qt Quick interface (qml/) over the daemon's control socket.
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
