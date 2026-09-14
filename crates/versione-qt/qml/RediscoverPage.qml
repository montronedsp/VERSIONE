import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: page
    required property var backend

    readonly property var candidate: JSON.parse(backend.rediscoverJson || "null")

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        Label { text: "Rediscover"; font.bold: true; font.pixelSize: 22 }
        Label { text: "One project at a time. Skip never destroys or downgrades a project." }

        Label {
            visible: candidate === null
            text: "No rediscovery candidates from the current library."
        }

        Label {
            visible: candidate !== null
            text: candidate ? candidate.card.title : ""
            font.bold: true
            font.pixelSize: 20
        }
        Label { visible: candidate !== null; text: candidate ? candidate.card.artists.join(", ") : "" }
        Label {
            visible: candidate !== null
            text: candidate ? ((candidate.card.bpm ? Math.round(candidate.card.bpm) + " BPM" : "—")
                               + " · " + (candidate.card.root || "") + " " + (candidate.card.scale || "")) : ""
        }
        Label {
            visible: candidate !== null
            text: candidate ? (candidate.card.version_count + " versions · last activity "
                               + candidate.card.last_activity) : ""
        }

        Label { visible: candidate !== null; text: "Rediscovered because:"; font.bold: true }
        Repeater {
            model: candidate ? candidate.reasons : []
            Label { text: "· " + modelData.explanation }
        }

        RowLayout {
            spacing: 8
            Button {
                visible: candidate && candidate.card.has_audio_preview
                text: "Play"
                onClicked: {
                    if (candidate) {
                        const idx = JSON.parse(backend.cardsJson).findIndex(
                            row => row.card.project_id === candidate.project_id)
                        if (idx >= 0) backend.play_card(idx)
                    }
                }
            }
            Button { text: "Move to Active"; onClicked: backend.move_rediscover_active() }
            Button { text: "Skip"; onClicked: backend.skip_rediscover() }
            Button { text: "Next"; onClicked: backend.next_rediscover() }
        }
    }
}
