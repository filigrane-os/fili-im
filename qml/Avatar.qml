// Round profile picture of a conversation (asked to the daemon when first shown), the initial on a disc until then or
// when there is none: the kit's FiliAvatar fed with the picture the daemon wrote.
import QtQuick
import Fili.Im
import Fili.UI

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

    FiliAvatar {
        anchors.fill: parent
        name: av.title
        size: av.size
        source: av.path !== "" ? "file://" + encodeURI(av.path) : ""
    }
}
