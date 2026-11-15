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
    property var alts: []              // other conversations of the same person: any picture will do
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
    MultiEffect {
        anchors.fill: parent
        visible: pic.status === Image.Ready
        source: pic
        maskEnabled: true
        maskSource: disc
    }
}
