import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: page
    required property var backend
    clip: true

    ColumnLayout {
        width: page.width - 24
        spacing: 10

        Label { text: "Insights"; font.bold: true; font.pixelSize: 22 }
        Label { text: "Local, private, factual. No telemetry. No gamification." }
        TextArea {
            Layout.fillWidth: true
            readOnly: true
            text: backend.overviewText
            wrapMode: TextArea.Wrap
            background: Item {}
        }
    }
}
