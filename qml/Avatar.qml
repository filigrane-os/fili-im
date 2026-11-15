// Round profile picture of a conversation (asked to the daemon when first shown), the initial on a disc until then or
// when there is none.
import QtQuick
import QtQuick.Effects
import Fili.Im

Item {
    id: av
    property int chatId: 0
    property string title: ""
    property int size: 32
    // Platform of the conversation (a small mark in the corner) when several are in use
    property string platform: ""
    property var platforms: []         // several: overlapping marks
    property var alts: []              // other conversations of the same person: any picture will do
    readonly property var marks: platforms.length > 0 ? platforms : (platform !== "" ? [platform] : [])
    readonly property string path: {
        for (const id of [chatId].concat(alts)) if (id && win.avatars[id]) return win.avatars[id];
        return "";
    }
    width: size
    height: size
    function want() { if (chatId) win.wantAvatar(chatId); alts.forEach(a => win.wantAvatar(a)); }
    Component.onCompleted: want()
    onChatIdChanged: want()
    onAltsChanged: want()

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: FiliTheme.surfaceActive
        Text {
            anchors.centerIn: parent
            text: av.title.trim().charAt(0).toUpperCase()
            textFormat: Text.PlainText
            font.family: FiliTheme.font
            font.pixelSize: Math.round(av.size * 0.42)
            font.weight: FiliTheme.weightAccent
            color: FiliTheme.textSecondary
        }
    }
    Image {
        id: pic
        anchors.fill: parent
        visible: false
        layer.enabled: true
        source: av.path !== "" ? "file://" + encodeURI(av.path) : ""
        sourceSize: Qt.size(av.size * 2, av.size * 2)
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
    }
    Rectangle {
        id: disc
        anchors.fill: parent
        radius: width / 2
        visible: false
        layer.enabled: true
    }
    Row {
        anchors { right: parent.right; bottom: parent.bottom; rightMargin: -2; bottomMargin: -2 }
        layoutDirection: Qt.RightToLeft
        spacing: -Math.round(av.size * 0.12)
        z: 2
        Repeater {
            model: av.marks
            Rectangle {
                required property string modelData
                width: Math.max(12, Math.round(av.size * 0.38))
                height: width
                radius: width / 2
                color: win.platformColor(modelData)
                border.width: 2
                border.color: FiliTheme.background
                Text { anchors.centerIn: parent; text: win.platformName(modelData).charAt(0); font.pixelSize: Math.round(parent.width * 0.6); font.bold: true; color: "white" }
            }
        }
    }
    MultiEffect {
        anchors.fill: parent
        visible: pic.status === Image.Ready
        source: pic
        maskEnabled: true
        maskSource: disc
    }
}
