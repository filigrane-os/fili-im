// Emoji picker (the list of the keyboard panel): a search field and a grid; `picked(emoji)`, Esc or a click outside closes.
import QtQuick
import Fili.Im

Rectangle {
    id: card
    z: 215
    visible: false
    color: FiliTheme.alpha(FiliTheme.backgroundDeep, 0.7)
    signal picked(string emoji)
    property var all: []
    readonly property var found: {
        const words = field.text.toLowerCase().split(/\s+/).filter(w => w);
        const out = [];
        for (const e of all) {
            if (words.every(w => e.name.includes(w))) { out.push(e); if (out.length >= 96) break; }
        }
        return out;
    }
    function show() {
        if (all.length === 0)
            all = Conversations.emojis().split("\n").filter(l => l).map(l => { const i = l.indexOf(" "); return { c: l.slice(0, i), name: l.slice(i + 1).toLowerCase() }; });
        field.text = "";
        visible = true;
        field.forceActiveFocus();
    }
    function choose(e) { visible = false; picked(e); }
    MouseArea { anchors.fill: parent; onClicked: card.visible = false }
    Rectangle {
        anchors.centerIn: parent
        width: Math.min(400, card.width - 2 * FiliTheme.margin)
        height: Math.min(360, card.height - 2 * FiliTheme.margin)
        radius: FiliTheme.radiusWindow
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: FiliTheme.border
        MouseArea { anchors.fill: parent }
        SearchField {
            id: field
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: FiliTheme.space3 }
            placeholderText: Conversations.tr("Search emoji")
            Keys.onEscapePressed: card.visible = false
            Keys.onReturnPressed: if (card.found.length) card.choose(card.found[0].c)
            Keys.onEnterPressed: if (card.found.length) card.choose(card.found[0].c)
        }
        GridView {
            id: grid
            anchors { left: parent.left; right: parent.right; top: field.bottom; bottom: parent.bottom; margins: FiliTheme.space3 }
            cellWidth: 36
            cellHeight: 36
            clip: true
            model: card.found
            boundsBehavior: Flickable.StopAtBounds
            KineticScroll {}
            delegate: Rectangle {
                required property var modelData
                width: 36
                height: 36
                radius: FiliTheme.radiusControl
                color: area.containsMouse ? FiliTheme.surfaceHover : "transparent"
                Text { anchors.centerIn: parent; text: modelData.c; font.family: "Noto Color Emoji"; font.pixelSize: 22 }
                MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; onClicked: card.choose(modelData.c) }
            }
        }
        FiliScrollBar { flick: grid; anchors { top: grid.top; bottom: grid.bottom; right: grid.right } }
    }
}
