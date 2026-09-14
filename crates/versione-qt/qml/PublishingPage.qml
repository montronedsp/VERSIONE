import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: page
    required property var backend
    clip: true

    readonly property var board: JSON.parse(backend.pipelineJson || "null")

    ColumnLayout {
        width: page.width - 24
        spacing: 10

        Label { text: "Publishing"; font.bold: true; font.pixelSize: 22 }

        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            Repeater {
                model: board ? board.stages : []
                GroupBox {
                    title: modelData.stage
                    Layout.preferredWidth: 160
                    ColumnLayout {
                        Repeater {
                            model: modelData.projects
                            RowLayout {
                                Label { text: modelData.title; Layout.fillWidth: true }
                                Button {
                                    text: "Open"
                                    onClicked: backend.open_pipeline_project(modelData.project_id)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
