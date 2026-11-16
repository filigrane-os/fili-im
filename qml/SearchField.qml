// Search or filter field: a × on the right, as soon as there is text, clears it (Esc does the same through `cleared`).
import QtQuick
import Fili.Im

Field {
    id: f
    signal cleared()
    rightPadding: clear.visible ? 26 : FiliTheme.space2
    function clearAll() { text = ""; f.cleared(); }
    Rectangle {
        id: clear
        visible: f.text !== ""
        anchors { right: parent.right; rightMargin: 4; verticalCenter: parent.verticalCenter }
        width: 18
        height: 18
        radius: 9
        color: clearArea.containsMouse ? FiliTheme.surfaceHover : "transparent"
        Icon { anchors.centerIn: parent; name: "x"; size: 12; color: FiliTheme.textSecondary }
        MouseArea { id: clearArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { f.clearAll(); f.forceActiveFocus(); } }
    }
}
