import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: page
    required property var backend
    clip: true

    ColumnLayout {
        width: page.width - 24
        spacing: 12

        Label { text: "Settings"; font.bold: true; font.pixelSize: 22 }

        GroupBox {
            title: "Privacy"
            Layout.fillWidth: true
            ColumnLayout {
                Label {
                    wrapMode: Text.Wrap
                    text: "Your project library and Insights stay on this computer.\n"
                          + "VERSIONE does not send analytics or project metadata to a VERSIONE service.\n"
                          + "Remote project content is uploaded only to storage you configure."
                }
                CheckBox {
                    text: "I understand VERSIONE is local-first"
                    checked: backend.privacyAck
                    onCheckedChanged: backend.updatePrivacyAck(checked)
                }
            }
        }

        Label { text: "Library index: ~/.versione/library.toml (or VERSIONE_LIBRARY)" }
        Label { text: "Credentials for remote storage come from environment variables only." }

        RowLayout {
            Button { text: "Rebuild library index"; onClicked: backend.rebuild_index() }
            Button { text: "Add project…"; onClicked: backend.add_project() }
        }
    }
}
