// Quick switcher (Ctrl+K, Ctrl+N): type a name, Enter opens the first match, arrows move, Esc closes.
import QtQuick
import Fili.Im

Rectangle {
    id: card
    z: 215
    visible: false
    color: FiliTheme.alpha(FiliTheme.backgroundDeep, 0.7)
    property int index: 0
    readonly property var matches: {
        const q = field.text.trim().toLowerCase();
        const all = win.chats;
        return (q === "" ? all : all.filter(c => c.title.toLowerCase().includes(q))).slice(0, 8);
    }
    function show() { field.text = ""; index = 0; visible = true; field.forceActiveFocus(); }
    function choose(i) {
        if (i >= 0 && i < matches.length) { win.openChat(matches[i].id); visible = false; }
    }
    MouseArea { anchors.fill: parent; onClicked: card.visible = false }
    Rectangle {
        anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: parent.height / 6 }
        width: Math.min(460, card.width - 2 * FiliTheme.margin)
        height: list.height + field.height + 3 * FiliTheme.space3
        radius: FiliTheme.radiusWindow
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: FiliTheme.border
        MouseArea { anchors.fill: parent }
        Field {
            id: field
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: FiliTheme.space3 }
            placeholderText: Conversations.tr("Go to a conversation")
            onTextChanged: card.index = 0
            Keys.onEscapePressed: card.visible = false
            Keys.onReturnPressed: card.choose(card.index)
            Keys.onEnterPressed: card.choose(card.index)
            Keys.onDownPressed: card.index = Math.min(card.matches.length - 1, card.index + 1)
            Keys.onUpPressed: card.index = Math.max(0, card.index - 1)
        }
        Column {
            id: list
            anchors { left: parent.left; right: parent.right; top: field.bottom; margins: FiliTheme.space3; topMargin: FiliTheme.space2 }
            Repeater {
                model: card.matches
                Rectangle {
                    required property var modelData
                    required property int index
                    width: list.width
                    height: 28
                    radius: FiliTheme.radiusControl
                    color: index === card.index ? FiliTheme.surfaceHover : "transparent"
                    Text {
                        anchors { left: parent.left; leftMargin: FiliTheme.space2; right: unread.left; verticalCenter: parent.verticalCenter }
                        text: modelData.title
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        font.family: FiliTheme.font
                        font.pixelSize: FiliTheme.size
                        color: FiliTheme.text
                    }
                    Text {
                        id: unread
                        anchors { right: parent.right; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                        text: modelData.unread > 0 ? String(modelData.unread) : ""
                        font.family: FiliTheme.font
                        font.pixelSize: FiliTheme.sizeSmall
                        color: FiliTheme.muted
                    }
                    MouseArea { anchors.fill: parent; hoverEnabled: true; onEntered: card.index = index; onClicked: card.choose(index) }
                }
            }
        }
    }
}
