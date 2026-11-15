// Contact (or group) card: profile picture, what is known about it, actions, and the files shared in the conversation.
import QtQuick
import Fili.Im

Rectangle {
    id: card
    z: 210
    visible: false
    color: FiliTheme.alpha(FiliTheme.backgroundDeep, 0.7)
    focus: visible

    // What the card is about follows the view: one platform's conversation, or every conversation of the person
    property var ids: []
    readonly property var chats: ids.map(win.chatById).filter(c => c)
    readonly property var chat: chats.length ? chats[0] : null
    readonly property bool all: chats.length > 1
    property var media: []
    property string filter: "all"
    readonly property var shown: media.filter(m => filter === "all" || (filter === "visual" ? (m.media.kind === "image" || m.media.kind === "video" || m.media.kind === "sticker") : (m.media.kind !== "image" && m.media.kind !== "video" && m.media.kind !== "sticker")))
    readonly property bool group: chat ? chat.kind !== "direct" : false
    readonly property bool muted: chats.length > 0 && chats.every(c => c.muted)
    readonly property int unread: chats.reduce((n, c) => n + (c.unread > 0 ? c.unread : 0), 0)
    function phoneOf(c) {
        if (!c || c.kind !== "direct") return "";
        const m = /^(\d{6,15})@s\.whatsapp\.net$/.exec(c.native_id);
        return m ? "+" + m[1] : "";
    }
    function accountNameOf(c) {
        const a = c ? win.accountByKey(c.account) : null;
        return a ? (a.self_name || a.name) : "";
    }
    // One platform: its name, number and account; every platform: how many
    readonly property string subtitle: {
        if (!chat) return "";
        if (all) return Conversations.tr("%1 platforms").replace("%1", chats.length);
        return [group ? Conversations.tr("Group") : phoneOf(chat), win.multi ? win.platformName(win.platformOf(chat)) : "", accountNameOf(chat)].filter(x => x !== "").join(" · ");
    }

    function show() {
        ids = win.scopeIds();
        media = [];
        filter = "all";
        visible = true;
        const mine = ids.slice();
        let acc = [], waiting = mine.length;
        mine.forEach(id => win.rpc("chat.media", { chat: id, limit: 300 }, list => {
            acc = acc.concat(list);
            if (--waiting === 0 && card.ids.join() === mine.join()) card.media = acc.sort((a, b) => b.timestamp - a.timestamp);
        }, () => { --waiting; }));
    }
    function openMedia(m) { quickLookCard.open(media.slice().reverse(), m.id); }
    onVisibleChanged: if (visible) forceActiveFocus()
    Keys.onEscapePressed: visible = false
    MouseArea { anchors.fill: parent; onClicked: card.visible = false }

    Rectangle {
        anchors.centerIn: parent
        width: Math.min(460, card.width - 2 * FiliTheme.margin)
        height: Math.min(600, card.height - 2 * FiliTheme.margin)
        radius: FiliTheme.radiusWindow
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: FiliTheme.border
        MouseArea { anchors.fill: parent }

        Column {
            id: head
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: FiliTheme.space5 }
            spacing: FiliTheme.space2
            Avatar { anchors.horizontalCenter: parent.horizontalCenter; chatId: card.chat ? card.chat.id : 0; title: card.chat ? card.chat.title : ""; size: 96; alts: card.ids.slice(1) }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: card.chat ? card.chat.title : ""
                elide: Text.ElideRight
                textFormat: Text.PlainText
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.size + 3
                font.weight: FiliTheme.weightAccent
                color: FiliTheme.text
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: card.subtitle
                elide: Text.ElideRight
                textFormat: Text.PlainText
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeList
                color: FiliTheme.textSecondary
            }
            Text {
                width: parent.width
                visible: card.muted
                horizontalAlignment: Text.AlignHCenter
                text: Conversations.tr("Notifications are muted")
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: FiliTheme.muted
            }
            // Every platform: one line each (pick one to see its card and its conversation)
            Column {
                visible: card.all
                width: parent.width
                spacing: 2
                Repeater {
                    model: card.all ? card.chats : []
                    Rectangle {
                        required property var modelData
                        width: parent.width
                        height: 30
                        radius: FiliTheme.radiusControl
                        color: lineArea.containsMouse ? FiliTheme.surfaceHover : "transparent"
                        Rectangle {
                            id: lineBadge
                            anchors { left: parent.left; leftMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                            width: 18; height: 18; radius: 9
                            color: win.platformColor(win.platformOf(modelData))
                            Text { anchors.centerIn: parent; text: win.platformName(win.platformOf(modelData)).charAt(0); font.pixelSize: 11; font.bold: true; color: "white" }
                        }
                        Text {
                            anchors { left: lineBadge.right; leftMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                            text: win.platformName(win.platformOf(modelData))
                            font.family: FiliTheme.font
                            font.pixelSize: FiliTheme.size
                            color: FiliTheme.text
                        }
                        Text {
                            anchors { right: parent.right; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                            text: [card.phoneOf(modelData), card.accountNameOf(modelData), modelData.muted ? Conversations.tr("muted") : ""].filter(x => x !== "").join(" · ")
                            font.family: FiliTheme.font
                            font.pixelSize: FiliTheme.sizeList
                            color: FiliTheme.textSecondary
                        }
                        MouseArea { id: lineArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { win.setScope(modelData.id); card.show(); } }
                    }
                }
            }
            // One platform: the others, to go to
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: FiliTheme.space2
                visible: !card.all && win.grouped
                Text { anchors.verticalCenter: parent.verticalCenter; text: Conversations.tr("Also on"); font.family: FiliTheme.font; font.pixelSize: FiliTheme.sizeList; color: FiliTheme.textSecondary }
                Repeater {
                    model: win.grouped && !card.all ? win.group.filter(c => card.chat && c.id !== card.chat.id) : []
                    TextButton { required property var modelData; label: win.platformName(win.platformOf(modelData)); onClicked: { win.setScope(modelData.id); card.show(); } }
                }
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: FiliTheme.space2
                TextButton {
                    label: card.muted ? Conversations.tr("Unmute") : Conversations.tr("Mute…")
                    onClicked: {
                        if (card.muted) card.chats.forEach(c => win.rpc("chat.mute", { chat: c.id, until: 0 }));
                        else { const p = mapToItem(card, 0, height); muteMenu.openAt(p.x, p.y, [
                            { id: "3600000", label: Conversations.tr("1 hour") }, { id: "28800000", label: Conversations.tr("8 hours") },
                            { id: "604800000", label: Conversations.tr("1 week") }, { id: "-1", label: Conversations.tr("Always") }]); }
                    }
                }
                TextButton {
                    visible: card.unread > 0
                    label: Conversations.tr("Mark as Read")
                    onClicked: card.chats.forEach(c => { if (c.unread > 0) win.rpc("mark_read", { chat: c.id, receipt: true }); })
                }
            }
            Row {
                spacing: FiliTheme.space2
                Text { anchors.verticalCenter: parent.verticalCenter; text: Conversations.tr("Shared files"); font.family: FiliTheme.font; font.pixelSize: FiliTheme.sizeList; font.weight: FiliTheme.weightAccent; color: FiliTheme.text }
                TextButton { label: Conversations.tr("All files"); primary: card.filter === "all"; onClicked: card.filter = "all" }
                TextButton { label: Conversations.tr("Photos and videos"); primary: card.filter === "visual"; onClicked: card.filter = "visual" }
                TextButton { label: Conversations.tr("Documents"); primary: card.filter === "docs"; onClicked: card.filter = "docs" }
            }
        }

        GridView {
            id: grid
            anchors { left: parent.left; right: parent.right; top: head.bottom; bottom: parent.bottom; margins: FiliTheme.space5; topMargin: FiliTheme.space3 }
            clip: true
            cellWidth: Math.floor(width / Math.max(3, Math.floor(width / 92)))
            cellHeight: cellWidth
            model: card.shown
            boundsBehavior: Flickable.StopAtBounds
            KineticScroll {}
            delegate: Item {
                id: cell
                required property var modelData
                width: grid.cellWidth
                height: grid.cellHeight
                property string path: ""
                property string thumb: ""
                readonly property bool picture: modelData.media.kind === "image" || modelData.media.kind === "sticker"
                readonly property bool video: modelData.media.kind === "video"
                Component.onCompleted: {
                    if (video) win.rpc("media.thumb", { message: modelData.id }, r => { thumb = r.path; }, () => { });
                    else if (picture && (modelData.media.size || 0) <= 10000000)
                        win.rpc("media.fetch", { message: modelData.id }, r => { path = r.path; }, () => { });
                }
                Rectangle {
                    anchors { fill: parent; margins: 2 }
                    radius: FiliTheme.radiusControl
                    color: FiliTheme.backgroundDeep
                    clip: true
                    Image {
                        anchors.fill: parent
                        source: cell.picture && cell.path !== "" ? "file://" + encodeURI(cell.path) : cell.video && cell.thumb !== "" ? "file://" + encodeURI(cell.thumb) : ""
                        sourceSize.width: 240
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                    }
                    Text {
                        anchors.centerIn: parent
                        visible: !(cell.picture || cell.video) || (cell.picture ? cell.path === "" : cell.thumb === "")
                        text: cell.video ? "▶" : cell.picture ? "▣" : "▤"
                        font.pixelSize: 22
                        color: FiliTheme.muted
                    }
                    Text {
                        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 3 }
                        visible: !(cell.picture || cell.video)
                        text: cell.modelData.media.name || ""
                        elide: Text.ElideMiddle
                        horizontalAlignment: Text.AlignHCenter
                        textFormat: Text.PlainText
                        font.family: FiliTheme.font
                        font.pixelSize: FiliTheme.sizeSmall
                        color: FiliTheme.textSecondary
                    }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: card.openMedia(cell.modelData) }
                }
            }
        }
        FiliScrollBar { flick: grid; anchors { top: grid.top; bottom: grid.bottom; right: grid.right } }
        Text {
            anchors { horizontalCenter: parent.horizontalCenter; top: grid.top; topMargin: FiliTheme.space4 }
            visible: card.shown.length === 0
            text: Conversations.tr("No shared file")
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeList
            color: FiliTheme.muted
        }
    }
    PopupMenu {
        id: muteMenu
        anchors.fill: parent
        z: 230
        onActivated: id => card.chats.forEach(c => win.rpc("chat.mute", { chat: c.id, until: id === "-1" ? -1 : Date.now() + parseInt(id) }))
    }
}
