import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: page
    required property var backend
    clip: true

    readonly property var board: JSON.parse(backend.focusJson || "null")

    ColumnLayout {
        width: page.width - 24
        anchors.margins: 12
        spacing: 10

        Label { text: "Focus"; font.bold: true; font.pixelSize: 22 }
        Label { text: "Evidence-based queues for finishing work already in the library." }

        Repeater {
            model: board ? [
                ["Active", board.active],
                ["Needs attention", board.needs_attention],
                ["Almost finished", board.almost_finished],
                ["Dormant", board.dormant],
                ["Publishing candidates", board.publishing_candidates]
            ] : []
            GroupBox {
                Layout.fillWidth: true
                title: modelData[0] + " (" + modelData[1].length + ")"
                ColumnLayout {
                    Repeater {
                        model: modelData[1].slice(0, 12)
                        Label {
                            text: "  " + modelData.title + " · "
                                  + (modelData.lifecycle || "unset") + " · "
                                  + (modelData.bpm ? Math.round(modelData.bpm) + " BPM" : "—")
                        }
                    }
                }
            }
        }

        GroupBox {
            Layout.fillWidth: true
            title: "Open tasks"
            visible: board && board.open_tasks
            ColumnLayout {
                Repeater {
                    model: board ? board.open_tasks : []
                    Label {
                        text: modelData.project_title + " — " + modelData.task.text + " · " + modelData.root_path
                        wrapMode: Text.Wrap
                    }
                }
            }
        }
    }
}
