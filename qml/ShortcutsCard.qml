// Keyboard shortcuts of Conversations (F1): everything can be done without the mouse; this card lists how.
import QtQuick
import Fili.Im

Rectangle {
    id: card
    z: 210
    visible: false
    color: FiliTheme.alpha(FiliTheme.backgroundDeep, 0.7)
    focus: visible

    readonly property var groups: [
        { title: Conversations.tr("Moving around"), rows: [
            ["Ctrl+K  Ctrl+N", Conversations.tr("Go to a conversation")],
            ["Alt+↓  Alt+↑", Conversations.tr("Next / previous conversation")],
            ["Ctrl+Tab  Ctrl+Shift+Tab", Conversations.tr("Next / previous tab")],
            ["Ctrl+F", Conversations.tr("Search every message (Esc clears)")],
            ["Ctrl+U", Conversations.tr("Unread conversations only")],
            ["F5  Ctrl+R", Conversations.tr("Refresh")],
            ["Ctrl+Shift+W", Conversations.tr("Window actions")]
        ] },
        { title: Conversations.tr("Writing"), rows: [
            ["Ctrl+L", Conversations.tr("Write a message")],
            ["Enter  Shift+Enter", Conversations.tr("Send, new line")],
            ["↑", Conversations.tr("Edit your last message (empty field)")],
            ["Ctrl+Shift+U", Conversations.tr("Send files")],
            ["Ctrl+V", Conversations.tr("Paste a picture or files")],
            ["Esc", Conversations.tr("Drop the reply or the edit")]
        ] },
        { title: Conversations.tr("Other"), rows: [
            ["Ctrl+W", Conversations.tr("Close the tab")],
            ["F1  Ctrl+/", Conversations.tr("This list")],
            ["Ctrl+Q", Conversations.tr("Quit")]
        ] }
    ]

    function show() { visible = true; }
    onVisibleChanged: if (visible) forceActiveFocus()
    Keys.onPressed: e => {
        if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Escape || e.key === Qt.Key_F1) {
            visible = false;
            e.accepted = true;
        }
    }
    MouseArea { anchors.fill: parent; onClicked: card.visible = false }

    Rectangle {
        anchors.centerIn: parent
        width: Math.min(620, card.width - 2 * FiliTheme.margin)
        height: Math.min(card.height - 2 * FiliTheme.margin, flick.contentHeight + 2 * FiliTheme.space5)
        radius: FiliTheme.radiusWindow
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: FiliTheme.border
        MouseArea { anchors.fill: parent }
        Flickable {
            id: flick
            anchors { fill: parent; margins: FiliTheme.space5 }
            contentHeight: column.height
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            KineticScroll {}
            Column {
                id: column
                width: flick.width
                spacing: FiliTheme.space2
                Text {
                    text: Conversations.tr("Keyboard shortcuts")
                    font.family: FiliTheme.font
                    font.pixelSize: FiliTheme.sizeTitle
                    font.weight: FiliTheme.weightAccent
                    color: FiliTheme.text
                }
                Repeater {
                    model: card.groups
                    Column {
                        required property var modelData
                        width: column.width
                        spacing: 4
                        Item { width: 1; height: FiliTheme.space2 }
                        Text { text: modelData.title; font.family: FiliTheme.font; font.pixelSize: FiliTheme.sizeSmall; font.weight: FiliTheme.weightAccent; color: FiliTheme.muted }
                        Repeater {
                            model: modelData.rows
                            Row {
                                required property var modelData
                                width: column.width
                                spacing: FiliTheme.space3
                                Text { width: 250; text: modelData[0]; font.family: FiliTheme.mono; font.pixelSize: FiliTheme.sizeList; color: FiliTheme.accentColor; elide: Text.ElideRight }
                                Text { width: parent.width - 250 - FiliTheme.space3; text: modelData[1]; font.family: FiliTheme.font; font.pixelSize: FiliTheme.sizeList; color: FiliTheme.text; wrapMode: Text.WordWrap }
                            }
                        }
                    }
                }
            }
        }
    }
}
