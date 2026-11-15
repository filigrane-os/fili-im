// Open conversations as document tabs (Facteur's look): band 40, tab 28, shown from the first open conversation.
import QtQuick
import Fili.Im

Rectangle {
    visible: win.tabs.length > 0
    height: visible ? FiliTheme.tabBarHeight : 0
    color: FiliTheme.background
    Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: FiliTheme.borderWidth; color: FiliTheme.border }
    Row {
        anchors { left: parent.left; leftMargin: FiliTheme.margin; verticalCenter: parent.verticalCenter }
        spacing: FiliTheme.space3
        Repeater {
            model: win.tabs
            Rectangle {
                required property int modelData
                readonly property var chat: win.chatById(modelData)
                height: FiliTheme.tabHeight
                width: Math.max(80, Math.min(180, label.implicitWidth + close.width + 3 * FiliTheme.space4))
                radius: FiliTheme.radiusControl
                color: modelData === win.current ? FiliTheme.surfaceActive : tabArea.containsMouse ? FiliTheme.surfaceHover : "transparent"
                Text {
                    id: label
                    anchors { left: parent.left; leftMargin: FiliTheme.space4; right: close.left; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                    text: (chat ? chat.title : "") + (chat && chat.unread > 0 ? " (" + chat.unread + ")" : "")
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    font.family: FiliTheme.font
                    font.pixelSize: FiliTheme.sizeList
                    font.weight: modelData === win.current ? FiliTheme.weightAccent : Font.Normal
                    color: modelData === win.current ? FiliTheme.text : FiliTheme.textSecondary
                }
                MouseArea {
                    id: tabArea
                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                    onClicked: mouse => mouse.button === Qt.MiddleButton ? win.closeTab(modelData) : win.openChat(modelData)
                }
                Text {
                    id: close
                    anchors { right: parent.right; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                    text: "×"
                    visible: tabArea.containsMouse || modelData === win.current
                    font.pixelSize: FiliTheme.size
                    color: closeArea.containsMouse ? FiliTheme.text : FiliTheme.muted
                    width: visible ? implicitWidth : 0
                    MouseArea { id: closeArea; anchors { fill: parent; margins: -4 } hoverEnabled: true; onClicked: win.closeTab(modelData) }
                }
            }
        }
    }
}
