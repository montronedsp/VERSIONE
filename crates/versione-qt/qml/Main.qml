import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.montronedsp.versione

ApplicationWindow {
    id: root
    width: 1280
    height: 800
    minimumWidth: 960
    minimumHeight: 640
    visible: true
    title: "VERSIONE"
    color: "#f6f4ef"

    VersioneBackend {
        id: backend
    }

    Component.onCompleted: backend.reload()

    header: ToolBar {
        background: Rectangle { color: "#f0ece4" }
        RowLayout {
            anchors.fill: parent
            anchors.margins: 8
            spacing: 12

            Label {
                text: "VERSIONE"
                font.pixelSize: 26
                font.bold: true
            }
            Label {
                text: "creative library"
                font.pixelSize: 15
                color: "#5a5a56"
            }
            Item { Layout.fillWidth: true }
            Button { text: "Reload"; onClicked: backend.reload() }
        }
    }

    footer: ToolBar {
        background: Rectangle { color: "#ebe7df" }
        RowLayout {
            anchors.fill: parent
            anchors.margins: 6
            Label {
                text: backend.status
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
            Button {
                text: backend.playing ? "Pause" : "Resume"
                onClicked: backend.playing ? backend.pause_audio() : backend.resume_audio()
            }
            Button { text: "Stop"; onClicked: backend.stop_audio() }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        TabBar {
            id: tabs
            Layout.fillWidth: true
            background: Rectangle { color: "#f0ece4" }

            Repeater {
                model: ["Library", "Focus", "Rediscover", "Publishing", "Insights", "Settings"]
                TabButton {
                    text: modelData
                    onClicked: backend.navigateTo(modelData)
                }
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: {
                switch (backend.nav) {
                case "Focus": return 1
                case "Rediscover": return 2
                case "Publishing": return 3
                case "Insights": return 4
                case "Settings": return 5
                default: return 0
                }
            }

            LibraryPage { backend: backend }
            FocusPage { backend: backend }
            RediscoverPage { backend: backend }
            PublishingPage { backend: backend }
            InsightsPage { backend: backend }
            SettingsPage { backend: backend }
        }
    }
}
