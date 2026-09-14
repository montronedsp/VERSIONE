use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("com.montronedsp.versione")
            .qml_file("qml/Main.qml")
            .qml_file("qml/LibraryPage.qml")
            .qml_file("qml/FocusPage.qml")
            .qml_file("qml/PublishingPage.qml")
            .qml_file("qml/InsightsPage.qml")
            .qml_file("qml/RediscoverPage.qml")
            .qml_file("qml/SettingsPage.qml"),
    )
    .qt_module("Network")
    .files(["src/backend.rs"])
    .build();
}
