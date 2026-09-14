import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: page
    required property var backend

    readonly property var cards: JSON.parse(backend.cardsJson || "[]")
    readonly property var detail: JSON.parse(backend.detailJson || "null")
    readonly property var header: JSON.parse(backend.overviewHeaderJson || "null")

    RowLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 12

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 8

            RowLayout {
                Layout.fillWidth: true
                visible: header !== null
                Label {
                    text: header ? header.projects + " Projects" : ""
                    font.bold: true
                    font.pixelSize: 20
                }
                Label {
                    text: header ? (header.active + " Active · " + header.creative_pool + " Creative Pool · "
                                    + header.publishing + " Publishing · " + header.released + " Released · "
                                    + header.archived + " Archived") : ""
                    Layout.fillWidth: true
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Label { text: "Search" }
                TextField {
                    Layout.preferredWidth: 280
                    placeholderText: "135 F phrygian Crossing Avenue"
                    text: backend.search
                    onTextEdited: backend.updateSearch(text)
                }
                ComboBox {
                    id: lifeFilter
                    model: ["any", "draft", "creative-pool", "active", "publishing", "released", "archived"]
                    currentIndex: Math.max(0, model.indexOf(backend.filterLifecycle))
                    onActivated: backend.updateFilterLifecycle(model[currentIndex])
                }
                ComboBox {
                    id: sortBox
                    model: ["Recent", "Oldest", "Title", "Bpm", "Lifecycle", "Versions"]
                    currentIndex: Math.max(0, model.indexOf(backend.sortMode))
                    onActivated: backend.updateSortMode(model[currentIndex])
                }
                Button {
                    text: "List"
                    checkable: true
                    checked: backend.layoutMode === "List"
                    onClicked: backend.updateLayoutMode("List")
                }
                Button {
                    text: "Grid"
                    checkable: true
                    checked: backend.layoutMode === "Grid"
                    onClicked: backend.updateLayoutMode("Grid")
                }
                Button { text: "Add folder…"; onClicked: backend.add_folder() }
            }

            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: cards
                spacing: 4
                delegate: ItemDelegate {
                    width: ListView.view.width
                    height: 72
                    highlighted: detail && detail.root_path === modelData.card.root_path
                    onClicked: backend.select_card(modelData.index)

                    RowLayout {
                        anchors.fill: parent
                        anchors.margins: 8
                        ColumnLayout {
                            Layout.fillWidth: true
                            Label { text: modelData.card.title; font.bold: true; font.pixelSize: 15 }
                            Label {
                                text: (modelData.card.artists[0] || "") + " · "
                                      + (modelData.card.genre || "—") + " · "
                                      + (modelData.card.bpm ? Math.round(modelData.card.bpm) + " BPM" : "—")
                                      + " " + ((modelData.card.root || "") + " " + (modelData.card.scale || ""))
                            }
                            Label {
                                text: (modelData.card.lifecycle || "unset") + " · V"
                                      + String(modelData.card.version_count).padStart(2, "0")
                                      + " · " + modelData.card.unfinished_tasks + " open tasks"
                            }
                        }
                        Button {
                            visible: modelData.card.has_audio_preview
                            text: "Play"
                            onClicked: backend.play_card(modelData.index)
                        }
                    }
                }
            }
        }

        Frame {
            Layout.preferredWidth: 360
            Layout.fillHeight: true
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 10
                spacing: 8
                visible: detail !== null

                Label { text: detail ? detail.title : ""; font.bold: true; font.pixelSize: 18 }
                Label { text: detail ? detail.root_path : ""; wrapMode: Text.Wrap; font.pixelSize: 11 }

                Label { text: "Title" }
                TextField {
                    Layout.fillWidth: true
                    text: detail ? detail.edit_title : ""
                    onTextEdited: backend.updateEditTitle(text)
                }
                Label { text: "BPM" }
                TextField {
                    Layout.fillWidth: true
                    text: detail ? detail.edit_bpm : ""
                    onTextEdited: backend.updateEditBpm(text)
                }
                Button { text: "Save identity"; onClicked: backend.save_identity() }

                Flow {
                    Layout.fillWidth: true
                    spacing: 4
                    Repeater {
                        model: ["draft", "creative-pool", "active", "publishing", "released", "archived"]
                        Button {
                            text: modelData
                            onClicked: backend.set_lifecycle(modelData)
                        }
                    }
                }

                Button {
                    visible: detail && detail.has_audio_preview
                    text: "Play preview"
                    onClicked: backend.play_selected()
                }

                Label {
                    visible: detail !== null
                    text: detail ? ("Versions: " + detail.version_count + " · Tasks open: " + detail.unfinished_tasks) : ""
                }

                Label { text: "Release checklist"; font.bold: true; visible: detail && detail.checklist.length > 0 }
                Repeater {
                    model: detail ? detail.checklist : []
                    Label { text: (modelData.complete ? "[x] " : "[ ] ") + modelData.label }
                }

                Item { Layout.fillHeight: true }
            }
            Label {
                anchors.centerIn: parent
                visible: detail === null
                text: "Select a project from the library."
            }
        }
    }
}
