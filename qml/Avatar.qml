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
    readonly property string path: chatId ? (win.avatars[chatId] || "") : ""
    width: size
    height: size
    Component.onCompleted: if (chatId) win.wantAvatar(chatId)
    onChatIdChanged: if (chatId) win.wantAvatar(chatId)

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
